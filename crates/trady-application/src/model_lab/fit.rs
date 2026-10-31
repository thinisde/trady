use super::*;

#[derive(Clone)]
struct Sample<'a> {
    row: &'a PointInTimeRow,
    x: Vec<f64>,
    class: usize,
    payoff: f64,
    allocation: f64,
    cash: f64,
}

fn sample(row: &PointInTimeRow) -> Result<Option<Sample<'_>>> {
    let (Some(class), Some(record)) = (row.outcome, row.execution.as_ref()) else {
        return Ok(None);
    };
    let mut x: Vec<_> = row
        .features
        .iter()
        .map(|v| number(*v))
        .collect::<Result<_>>()?;
    row.scenario.append(&mut x)?;
    Ok(Some(Sample {
        row,
        x,
        class: class.index(),
        payoff: record.net_per_acquisition()?,
        allocation: number(record.acquisition_cost.amount)? / number(record.opening_equity.amount)?,
        cash: number(record.cash_pnl.amount)? / number(record.opening_equity.amount)?,
    }))
}

fn counts(samples: &[Sample<'_>]) -> [u32; 4] {
    let mut n = [0; 4];
    for s in samples {
        n[s.class] += 1;
    }
    n
}

fn partition<'a>(
    data: &'a ObservationDataset,
    w: &Window,
    plan: &TrainingPlan,
) -> Result<(Vec<Sample<'a>>, u32, u32)> {
    let mut rows = Vec::new();
    let mut pending = 0;
    let mut purged = 0;
    for row in &data.rows {
        if row.decision_at < w.start || row.decision_at >= w.end {
            continue;
        }
        if row.outcome.is_none() || row.outcome_available_at > plan.fit_at {
            pending += 1;
        } else if row.feature_window_start_at < w.start
            || row
                .outcome_available_at
                .checked_add_signed(Duration::seconds(i64::from(plan.embargo_seconds)))
                .is_none_or(|end| end >= w.end)
        {
            purged += 1;
        } else if let Some(s) = sample(row)? {
            rows.push(s);
        }
    }
    Ok((rows, pending, purged))
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn softmax(w: &[Vec<f64>; 4], x: &[f64], temperature: f64) -> [f64; 4] {
    let logits: [f64; 4] = std::array::from_fn(|k| dot(&w[k], x) / temperature);
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exp = logits.map(|v| (v - max).exp());
    let total: f64 = exp.iter().sum();
    exp.map(|v| v / total)
}
fn standardize(x: &[f64], center: &[f64], scale: &[f64]) -> Vec<f64> {
    std::iter::once(1.0)
        .chain(
            x.iter()
                .zip(center)
                .zip(scale)
                .map(|((v, c), s)| (v - c) / s),
        )
        .collect()
}

fn ridge(
    samples: &[Sample<'_>],
    class: usize,
    center: &[f64],
    scale: &[f64],
    penalty: f64,
) -> Result<Vec<f64>> {
    let n = center.len() + 1;
    let mut matrix = vec![vec![0.0; n + 1]; n];
    let selected: Vec<_> = samples.iter().filter(|s| s.class == class).collect();
    for s in &selected {
        let x = standardize(&s.x, center, scale);
        for i in 0..n {
            for j in 0..n {
                matrix[i][j] += x[i] * x[j];
            }
            matrix[i][n] += x[i] * s.payoff;
        }
    }
    for (i, row) in matrix.iter_mut().enumerate() {
        row[i] += if i == 0 {
            1e-8
        } else {
            penalty * selected.len() as f64
        };
    }
    // Pivoted elimination: singular/ill-conditioned fits fail rather than emit NaN.
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|a, b| matrix[*a][col].abs().total_cmp(&matrix[*b][col].abs()))
            .ok_or_else(|| Error("empty conditional payoff solve".into()))?;
        matrix.swap(col, pivot);
        let d = matrix[col][col];
        require(
            d.is_finite() && d.abs() > 1e-12,
            "singular conditional payoff model",
        )?;
        for value in &mut matrix[col][col..=n] {
            *value /= d;
        }
        let normalized = matrix[col][col..=n].to_vec();
        for (i, row) in matrix.iter_mut().enumerate() {
            if i == col {
                continue;
            }
            let factor = row[col];
            for (value, pivot) in row[col..=n].iter_mut().zip(&normalized) {
                *value -= factor * pivot;
            }
        }
    }
    let w: Vec<_> = matrix.iter().map(|r| r[n]).collect();
    require(
        w.iter().all(|v| v.is_finite()),
        "non-finite conditional payoff fit",
    )?;
    Ok(w)
}

fn forecast(c: &ModelCandidate, s: &Sample<'_>) -> ([f64; 4], [f64; 4]) {
    let x = standardize(&s.x, &c.center, &c.scale);
    (
        softmax(&c.classifier, &x, c.temperature),
        std::array::from_fn(|k| dot(&c.conditional_payoffs[k], &x)),
    )
}
pub(crate) fn predict(
    c: &ModelCandidate,
    features: &[Decimal],
    scenario: &ScenarioFeatures,
) -> Result<([f64; 4], [f64; 4])> {
    c.validate()?;
    require(
        features.len() == c.feature_names.len(),
        "model feature schema mismatch",
    )?;
    let mut x: Vec<_> = features.iter().map(|v| number(*v)).collect::<Result<_>>()?;
    scenario.append(&mut x)?;
    require(
        in_range(c, &x) && c.supported_horizons.contains(&scenario.exchange_sessions),
        "model input exceeds evaluated support; abstain",
    )?;
    let x = standardize(&x, &c.center, &c.scale);
    let p = softmax(&c.classifier, &x, c.temperature);
    let r: [f64; 4] = std::array::from_fn(|k| dot(&c.conditional_payoffs[k], &x));
    require(
        p.iter().chain(&r).all(|v| v.is_finite())
            && r.iter().all(|v| (-2.0..=2.0).contains(v))
            && r[0] > 0.0
            && r[1] < 0.0
            && r[3] <= 1e-12,
        "conditional payoff model returned unsupported economics; abstain",
    )?;
    Ok((p, r))
}
fn in_range(c: &ModelCandidate, x: &[f64]) -> bool {
    x.len() == c.minimum.len()
        && x.iter()
            .zip(&c.minimum)
            .zip(&c.maximum)
            .all(|((v, min), max)| *v >= min - 1e-10 && *v <= max + 1e-10)
}

fn metrics(c: &ModelCandidate, samples: &[Sample<'_>]) -> Result<Metrics> {
    require(
        !samples.is_empty(),
        "no supported matured observations to evaluate",
    )?;
    let mut loss = [0.0; 4];
    let mut nll = 0.0;
    let mut absolute = 0.0;
    let mut realized = 0.0;
    let mut cash = 0.0;
    let mut selected = 0;
    let mut selected_sum = 0.0;
    let mut bins = [[(0_u32, 0.0, 0.0); 10]; 4];
    let mut regime_samples = BTreeMap::new();
    let mut horizon_samples = BTreeMap::new();
    for s in samples {
        let (p, r) = forecast(c, s);
        let expected: f64 = p.iter().zip(r).map(|(p, r)| p * r).sum();
        for k in 0..4 {
            let y = if k == s.class { 1.0 } else { 0.0 };
            loss[k] += (p[k] - y).powi(2);
            let bin = ((p[k] * 10.0).floor() as usize).min(9);
            bins[k][bin].0 += 1;
            bins[k][bin].1 += p[k];
            bins[k][bin].2 += y;
        }
        nll -= p[s.class].max(1e-15).ln();
        absolute += (s.payoff - expected).abs();
        realized += s.payoff * s.allocation;
        cash += s.cash;
        if expected > c.uncertainty_radius {
            selected += 1;
            selected_sum += s.payoff * s.allocation;
        }
        *regime_samples.entry(s.row.regime.clone()).or_default() += 1;
        *horizon_samples
            .entry(s.row.scenario.exchange_sessions)
            .or_default() += 1;
    }
    let n = samples.len() as f64;
    let reliability = std::array::from_fn(|k| {
        bins[k]
            .iter()
            .map(|(samples, p, y)| ReliabilityBin {
                samples: *samples,
                mean_probability: if *samples == 0 {
                    0.0
                } else {
                    p / f64::from(*samples)
                },
                observed_frequency: if *samples == 0 {
                    0.0
                } else {
                    y / f64::from(*samples)
                },
            })
            .collect()
    });
    Ok(Metrics {
        samples: samples.len() as u32,
        class_counts: counts(samples),
        multiclass_brier: loss.iter().sum::<f64>() / n,
        log_loss: nll / n,
        per_class_brier: loss.map(|v| v / n),
        reliability,
        mean_absolute_payoff_error: absolute / n,
        mean_realized_account_return: realized / n,
        selected_samples: selected,
        selected_mean_account_return: if selected > 0 {
            Some(selected_sum / f64::from(selected))
        } else {
            None
        },
        cash_account_return: cash / n,
        regime_samples,
        horizon_samples,
    })
}
fn uncertainty(c: &ModelCandidate, samples: &[Sample<'_>]) -> f64 {
    let mut residuals: Vec<_> = samples
        .iter()
        .map(|s| {
            let (p, r) = forecast(c, s);
            (s.payoff - p.iter().zip(r).map(|(p, r)| p * r).sum::<f64>()).abs()
        })
        .collect();
    residuals.sort_by(f64::total_cmp);
    let rank = ((samples.len() + 1) as f64 * c.plan.uncertainty_quantile).ceil() as usize;
    residuals[rank.saturating_sub(1).min(residuals.len() - 1)]
}
fn dummy_metrics() -> Metrics {
    Metrics {
        samples: 0,
        class_counts: [0; 4],
        multiclass_brier: 0.0,
        log_loss: 0.0,
        per_class_brier: [0.0; 4],
        reliability: std::array::from_fn(|_| Vec::new()),
        mean_absolute_payoff_error: 0.0,
        mean_realized_account_return: 0.0,
        selected_samples: 0,
        selected_mean_account_return: None,
        cash_account_return: 0.0,
        regime_samples: BTreeMap::new(),
        horizon_samples: BTreeMap::new(),
    }
}
fn acceptable(m: &Metrics, p: &TrainingPlan) -> bool {
    m.class_counts.iter().all(|n| *n >= p.minimum_class_samples)
        && m.multiclass_brier.is_finite()
        && m.multiclass_brier <= p.maximum_validation_brier
        && m.log_loss.is_finite()
        && m.log_loss <= p.maximum_validation_log_loss
}

pub fn fit(data: &ObservationDataset, plan: &TrainingPlan) -> Result<ModelCandidate> {
    data.validate()?;
    plan.validate(data.test_only)?;
    require(
        data.rows
            .iter()
            .all(|r| r.decision_at < plan.sealed_holdout_start_at),
        "sealed holdout must be a separate dataset, never passed to fitting",
    )?;
    let (train, p1, u1) = partition(data, &plan.train, plan)?;
    let (cal, p2, u2) = partition(data, &plan.calibration, plan)?;
    let (val, p3, u3) = partition(data, &plan.validation, plan)?;
    for rows in [&train, &cal, &val] {
        require(
            counts(rows)
                .iter()
                .all(|n| *n >= plan.minimum_class_samples),
            "insufficient matured per-outcome observations in chronological partition",
        )?;
    }
    let n = data.feature_names.len() + 4;
    let mut center = vec![0.0; n];
    let mut minimum = vec![f64::INFINITY; n];
    let mut maximum = vec![f64::NEG_INFINITY; n];
    for s in &train {
        for j in 0..n {
            center[j] += s.x[j] / train.len() as f64;
            minimum[j] = minimum[j].min(s.x[j]);
            maximum[j] = maximum[j].max(s.x[j]);
        }
    }
    let mut scale = vec![0.0; n];
    for s in &train {
        for j in 0..n {
            scale[j] += (s.x[j] - center[j]).powi(2) / train.len() as f64;
        }
    }
    for v in &mut scale {
        *v = v.sqrt().max(1e-9);
    }
    let inputs: Vec<_> = train
        .iter()
        .map(|s| standardize(&s.x, &center, &scale))
        .collect();
    let mut classifier: [Vec<f64>; 4] = std::array::from_fn(|_| vec![0.0; n + 1]);
    for _ in 0..plan.iterations {
        let mut grad: [Vec<f64>; 4] = std::array::from_fn(|_| vec![0.0; n + 1]);
        for (s, x) in train.iter().zip(&inputs) {
            let p = softmax(&classifier, x, 1.0);
            for k in 0..4 {
                for (j, v) in x.iter().enumerate() {
                    grad[k][j] +=
                        (p[k] - if s.class == k { 1.0 } else { 0.0 }) * v / train.len() as f64;
                }
            }
        }
        for k in 0..4 {
            for j in 0..=n {
                classifier[k][j] -= plan.learning_rate
                    * (grad[k][j]
                        + if j == 0 {
                            0.0
                        } else {
                            plan.ridge * classifier[k][j]
                        });
            }
        }
    }
    let mut payoffs: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::new());
    for (k, w) in payoffs.iter_mut().enumerate() {
        *w = ridge(&train, k, &center, &scale, plan.ridge)?;
    }
    let regimes: BTreeSet<_> = train.iter().map(|s| s.row.regime.clone()).collect();
    let supported_regimes: Vec<_> = regimes
        .into_iter()
        .filter(|r| {
            [&train, &cal, &val].iter().all(|rows| {
                rows.iter().filter(|s| s.row.regime == *r).count()
                    >= plan.minimum_regime_samples as usize
            })
        })
        .collect();
    let supported_horizons: Vec<_> = (1..=5)
        .filter(|h| {
            [&train, &cal, &val].iter().all(|rows| {
                rows.iter()
                    .filter(|s| s.row.scenario.exchange_sessions == *h)
                    .count()
                    >= plan.minimum_regime_samples as usize
            })
        })
        .collect();
    let mut c = ModelCandidate {
        schema_version: 1,
        format: FORMAT.into(),
        test_only: data.test_only,
        dataset_id: data.dataset_id.clone(),
        dataset_sha256: digest(data)?,
        feature_schema_version: data.feature_schema_version.clone(),
        feature_names: data.feature_names.clone(),
        label_version: data.label_version.clone(),
        cost_model_hash: data.cost_model_hash.clone(),
        plan: plan.clone(),
        center,
        scale,
        minimum,
        maximum,
        classifier,
        conditional_payoffs: payoffs,
        temperature: 1.0,
        uncertainty_radius: 0.0,
        supported_regimes,
        supported_horizons,
        training_rows: train.len() as u32,
        calibration: dummy_metrics(),
        validation: dummy_metrics(),
        pending_rows: p1 + p2 + p3,
        purged_rows: u1 + u2 + u3,
        development_eligible: false,
    };
    // Calibration sees only the separate calibration window; validation never fits parameters.
    let mut best = (f64::INFINITY, 1.0);
    for step in 0..=50 {
        c.temperature = 0.5 + f64::from(step) * 0.05;
        let nll = metrics(&c, &cal)?.log_loss;
        if nll < best.0 {
            best = (nll, c.temperature);
        }
    }
    c.temperature = best.1;
    c.uncertainty_radius = uncertainty(&c, &cal);
    c.calibration = metrics(&c, &cal)?;
    c.validation = metrics(&c, &val)?;
    c.development_eligible = acceptable(&c.validation, plan)
        && !c.supported_regimes.is_empty()
        && !c.supported_horizons.is_empty()
        && val.iter().chain(&cal).all(|s| in_range(&c, &s.x));
    c.validate()?;
    Ok(c)
}

/// Caller must durably claim the holdout with EvaluationJournal before this call.
/// No model/calibrator/payoff parameter is fitted from the independent test set.
pub fn evaluate(
    c: &ModelCandidate,
    data: &ObservationDataset,
    now: DateTime<Utc>,
) -> Result<EvaluationReport> {
    c.validate()?;
    data.validate()?;
    require(
        c.dataset_id != data.dataset_id
            && c.feature_schema_version == data.feature_schema_version
            && c.feature_names == data.feature_names
            && c.label_version == data.label_version
            && c.cost_model_hash == data.cost_model_hash
            && data.rows.iter().all(|r| {
                r.feature_window_start_at >= c.plan.sealed_holdout_start_at
                    && r.decision_at > c.plan.fit_at
            }),
        "independent holdout overlaps fit, features or schema",
    )?;
    let mut samples = Vec::new();
    let mut pending = 0;
    for row in &data.rows {
        if row.outcome_available_at > now {
            pending += 1;
            continue;
        }
        if let Some(s) = sample(row)? {
            if c.supported_regimes.contains(&row.regime)
                && predict(c, &row.features, &row.scenario).is_ok()
            {
                samples.push(s);
            } else {
                pending += 1;
            }
        } else {
            pending += 1;
        }
    }
    let aggregate = metrics(c, &samples)?;
    let mut by_regime = BTreeMap::new();
    let mut by_horizon = BTreeMap::new();
    for r in &c.supported_regimes {
        let rows: Vec<_> = samples
            .iter()
            .filter(|s| s.row.regime == *r)
            .cloned()
            .collect();
        if !rows.is_empty() {
            by_regime.insert(r.clone(), metrics(c, &rows)?);
        }
    }
    for h in &c.supported_horizons {
        let rows: Vec<_> = samples
            .iter()
            .filter(|s| s.row.scenario.exchange_sessions == *h)
            .cloned()
            .collect();
        if !rows.is_empty() {
            by_horizon.insert(*h, metrics(c, &rows)?);
        }
    }
    let eligible = c.development_eligible
        && pending == 0
        && acceptable(&aggregate, &c.plan)
        && c.supported_regimes.iter().all(|r| {
            by_regime.get(r).is_some_and(|m| {
                m.samples >= c.plan.minimum_regime_samples && acceptable(m, &c.plan)
            })
        })
        && c.supported_horizons.iter().all(|h| {
            by_horizon.get(h).is_some_and(|m| {
                m.samples >= c.plan.minimum_regime_samples && acceptable(m, &c.plan)
            })
        });
    Ok(EvaluationReport {
        schema_version: 1,
        candidate_sha256: c.hash()?,
        holdout_id: data.dataset_id.clone(),
        holdout_sha256: digest(data)?,
        test_only: c.test_only || data.test_only,
        evaluation_started_at: data
            .rows
            .first()
            .ok_or_else(|| Error("empty holdout".into()))?
            .decision_at,
        evaluation_ended_at: data
            .rows
            .iter()
            .map(|r| r.outcome_available_at)
            .max()
            .ok_or_else(|| Error("empty holdout".into()))?,
        evaluated_at: now,
        metrics: aggregate,
        by_regime,
        by_horizon,
        empirical_uncertainty_radius: uncertainty(c, &samples),
        pending_or_unsupported: pending,
        eligible,
    })
}
