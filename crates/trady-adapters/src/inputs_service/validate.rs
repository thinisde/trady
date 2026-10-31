use super::*;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::screening::{valid_gics_pair, valid_isin};
use trady_policy::{EntryCosts, fresh};

pub(super) fn quote(q: &Quote, now: DateTime<Utc>) -> Result<()> {
    require(
        q.bid > Decimal::ZERO
            && q.ask >= q.bid
            && q.observed_at <= q.received_at
            && fresh(q.observed_at, now, 60)
            && q.received_at <= now
            && q.tradable,
        "quote is stale, crossed, non-executable or future dated",
    )
}
fn charges(c: &EntryCosts, now: DateTime<Utc>) -> Result<()> {
    require(
        c.fixed.amount >= Decimal::ZERO
            && c.uncertainty.amount >= Decimal::ZERO
            && c.fixed.currency == c.uncertainty.currency
            && c.rate >= Decimal::ZERO
            && c.rate < Decimal::ONE,
        "invalid fee schedule or uncertainty",
    )?;
    if let Some(fx) = &c.fx {
        require(
            fx.from != fx.to
                && fx.to == c.fixed.currency
                && fx.rate > Decimal::ZERO
                && fresh(fx.observed_at, now, 60),
            "invalid, stale or future FX observation",
        )?;
    }
    Ok(())
}
pub(super) fn costs(c: &CostsPacket, currency: &Currency, now: DateTime<Utc>) -> Result<()> {
    require(
        fresh(c.observed_at, now, 60) && now < c.valid_until,
        "cost observation unavailable or expired",
    )?;
    require(
        c.costs.fixed.currency == *currency && c.costs.uncertainty.currency == *currency,
        "cost observation uses another account currency",
    )?;
    charges(&c.costs, now)
}
pub(super) fn session(
    p: &SessionPacket,
    a: &ServiceAccount,
    s: &CalendarSession,
    now: DateTime<Utc>,
) -> Result<()> {
    let m = p
        .session
        .as_ref()
        .ok_or_else(|| Error("active session accounting missing".into()))?;
    require(
        p.account_id == a.account_id
            && m.session_id == s.session_id
            && m.opens_at == s.opens_at
            && m.closes_at == s.closes_at
            && fresh(m.observed_at, now, 60)
            && [&m.opening_equity, &m.deposits, &m.withdrawals]
                .iter()
                .all(|v| v.currency == a.currency)
            && m.opening_equity.amount > Decimal::ZERO
            && m.deposits.amount >= Decimal::ZERO
            && m.withdrawals.amount >= Decimal::ZERO,
        "unverified session accounting or exchange times",
    )
}
pub(super) fn universe(u: &UniverseDocument, now: DateTime<Utc>) -> Result<()> {
    require(
        u.schema_version == 1
            && u.index_id == "SP500"
            && u.complete
            && valid_id(&u.snapshot_id)
            && valid_id(&u.classification_snapshot_id)
            && valid_id(&u.taxonomy_edition)
            && !u.provider.is_empty()
            && u.as_of <= u.available_at
            && u.available_at <= now
            && now < u.expires_at
            && fresh(u.as_of, now, 604_800)
            && !u.members.is_empty()
            && u.members.len() <= 2048,
        "invalid, incomplete or stale universe export",
    )?;
    let mut identities = BTreeSet::new();
    for row in &u.members {
        require(
            valid_id(&row.issuer_id)
                && valid_isin(&row.share_class_isin)
                && identities.insert((&row.share_class_isin, &row.listing_mic))
                && row.listing_mic.len() == 4
                && row
                    .listing_mic
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && !row.symbol.trim().is_empty()
                && row.symbol.len() <= 32
                && row.membership.valid()
                && row.classification.effective.valid()
                && row.classification.observed_at <= u.available_at
                && valid_gics_pair(
                    &row.classification.sector_code,
                    &row.classification.industry_group_code,
                ),
            "universe has invalid identity, effective interval or classification",
        )?;
    }
    Ok(())
}

pub(super) fn payload(
    config: &InputsServiceConfig,
    i: &SourceImport,
    now: DateTime<Utc>,
) -> Result<()> {
    let bounds = |observed, available, until| {
        require(
            observed == i.observed_at
                && available == i.available_at
                && until <= i.valid_until
                && observed <= available
                && available <= now
                && now < until,
            "payload chronology differs from immutable source envelope",
        )
    };
    match &i.payload {
        ImportPayload::Quote(q) => {
            quote(q, now)?;
            bounds(q.observed_at, q.received_at, i.valid_until)
        }
        ImportPayload::Costs(c) => {
            let account = config
                .accounts
                .first()
                .ok_or_else(|| Error("cost account currency unavailable".into()))?;
            costs(c, &account.currency, now)?;
            bounds(c.observed_at, i.available_at, c.valid_until)
        }
        ImportPayload::Calendar(c) => {
            c.validate(now)?;
            bounds(c.observed_at, c.available_at, c.valid_until)
        }
        ImportPayload::Universe(u) => {
            universe(u, now)?;
            require(
                u.provider == i.provider,
                "universe provider identity mismatch",
            )?;
            bounds(u.as_of, u.available_at, u.expires_at)
        }
        ImportPayload::Session(p) => {
            let account = config.account(&p.account_id)?;
            let m = p.session.as_ref().ok_or_else(|| {
                Error("import actual accounting; closure comes from calendar".into())
            })?;
            require(
                m.opens_at < m.closes_at
                    && m.opening_equity.currency == account.currency
                    && m.opening_equity.amount > Decimal::ZERO
                    && m.deposits.amount >= Decimal::ZERO
                    && m.withdrawals.amount >= Decimal::ZERO
                    && m.deposits.currency == account.currency
                    && m.withdrawals.currency == account.currency
                    && valid_id(&m.session_id),
                "invalid session accounting",
            )?;
            bounds(m.observed_at, i.available_at, i.valid_until)
        }
        ImportPayload::Evidence(e) => {
            let r = &e.record;
            config.account(&r.account_id)?;
            require(
                r.id == i.id
                    && valid_id(&r.kind)
                    && r.policy_version > 0
                    && r.revision_hash == digest(&e.artifact)?
                    && r.dependencies == i.dependencies
                    && !r.material_conflict
                    && (r.kind != "source"
                        || r.source_lineage.as_ref() == Some(&i.origin_lineage_id))
                    && (!r.primary_verified || r.kind == "source"),
                "unbound, conflicting or fabricated evidence artifact",
            )?;
            bounds(r.observed_at, i.available_at, r.valid_until)
        }
        ImportPayload::Research(p) => {
            let a = config.account(&p.account_id)?;
            require(
                p.schema_version == 1
                    && p.owner_id == a.owner_id
                    && p.policy_version == a.policy.version
                    && p.retain_until <= i.retain_until
                    && p.valid_until < p.retain_until
                    && fresh(p.observed_at, now, 300)
                    && p.proposals.len() <= 5
                    && config.sources[&i.source_id].rights.llm_processing
                    && p.screening_observations.broker_support.len() <= 2048
                    && p.screening_observations.markets.len() <= 2048
                    && p.screening_observations.priorities.len() <= 2048,
                "unscoped, stale, unpermitted or oversized research packet",
            )?;
            bounds(p.observed_at, i.available_at, p.valid_until)
        }
        ImportPayload::Forecast(f) => {
            config.account(&f.account_id)?;
            let o = &f.observation;
            require(
                valid_id(&o.market_snapshot_id)
                    && valid_id(&o.feature_schema_version)
                    && !o.features.is_empty()
                    && o.features.len() <= 24
                    && o.features.len() == o.feature_names.len()
                    && o.feature_names.iter().all(|v| valid_id(v))
                    && valid_id(&o.regime)
                    && o.available_at <= o.generated_at
                    && o.generated_at <= i.imported_at
                    && fresh(o.observed_at, now, 300),
                "unbound or unsupported forecast features",
            )?;
            bounds(o.observed_at, o.available_at, o.valid_until)
        }
    }
}

impl InputsService {
    pub(super) fn validate_research(
        &self,
        i: &SourceImport,
        p: &AnalystPacket,
        now: DateTime<Utc>,
    ) -> Result<()> {
        payload(self.config(), i, now)?;
        let account = self.config().account(&p.account_id)?;
        let calendar = self.calendar(&account.calendar_id, now)?;
        let deadline = calendar.deadline(
            now,
            account
                .formula_authority
                .active
                .parameters
                .max_holding_sessions,
        )?;
        let mut instruments = BTreeSet::new();
        for draft in &p.proposals {
            require(
                instruments.insert(&draft.instrument_id)
                    && draft.currency == draft.entry_range.currency
                    && draft.deadline_at == deadline
                    && draft.valid_until <= p.valid_until
                    && draft.valid_until > now
                    && draft.entry_range.minimum > Decimal::ZERO
                    && draft.entry_range.maximum >= draft.entry_range.minimum
                    && draft.features.len() <= 128
                    && valid_id(&draft.feature_schema_version)
                    && !draft.perpetual_review_reason.trim().is_empty()
                    && draft.perpetual_review_reason.len() <= 4000,
                "invalid research draft scope, deadline or limits",
            )?;
            charges(&draft.entry_costs, now)?;
            charges(&draft.exit_costs, now)?;
            require(
                draft.entry_costs.fixed.currency == account.currency
                    && draft.exit_costs.fixed.currency == account.currency,
                "research costs use another account currency",
            )?;
            let dossier = &draft.dossier;
            require(
                dossier.finalized
                    && dossier.blockers.is_empty()
                    && valid_id(&dossier.id)
                    && !dossier.records.is_empty()
                    && dossier.records.len() <= 128
                    && dossier.skills.len() <= 64,
                "research dossier is incomplete, blocked or oversized",
            )?;
            let mut records = BTreeMap::new();
            let mut original = BTreeMap::new();
            for record in &dossier.records {
                require(
                    records.insert(&record.id, record).is_none()
                        && i.dependencies.contains(&record.id),
                    "dossier has duplicate or unimported artifact",
                )?;
                let imported = self.store.resolve(&record.id, now)?;
                let ImportPayload::Evidence(e) = &imported.payload else {
                    return Err(Error("dossier references non-evidence input".into()));
                };
                require(
                    digest(record)? == digest(&e.record)?
                        && record.account_id == p.account_id
                        && record.instrument_id == draft.instrument_id
                        && record.policy_version == p.policy_version
                        && record.valid_until >= draft.valid_until
                        && p.retain_until <= imported.retain_until,
                    "dossier assertion differs from separately reviewed artifact or source rights",
                )?;
                original.insert(record.id.clone(), imported);
            }
            for record in &dossier.records {
                require(
                    record
                        .dependencies
                        .iter()
                        .all(|id| records.contains_key(id)),
                    "dossier omits dependency artifacts",
                )?;
            }
            let primary: Vec<_> = dossier
                .records
                .iter()
                .filter(|r| r.kind == "source" && r.primary_verified)
                .collect();
            require(
                !primary.is_empty() && draft.confidence_inputs.primary_verified,
                "verified primary facts are missing",
            )?;
            let groups: BTreeSet<_> = primary
                .iter()
                .map(|r| {
                    self.config().sources[&original[&r.id].source_id]
                        .independence_group
                        .as_str()
                })
                .collect();
            let lineages: BTreeSet<_> = primary
                .iter()
                .map(|r| original[&r.id].origin_lineage_id.as_str())
                .collect();
            let independent = independent_origins(
                dossier
                    .records
                    .iter()
                    .filter(|r| r.kind == "source" && !r.primary_verified)
                    .filter_map(|r| {
                        let origin = &original[&r.id];
                        let source = &self.config().sources[&origin.source_id];
                        if groups.contains(source.independence_group.as_str())
                            || lineages.contains(origin.origin_lineage_id.as_str())
                        {
                            None
                        } else {
                            Some((
                                source.independence_group.as_str(),
                                origin.origin_lineage_id.as_str(),
                            ))
                        }
                    }),
            );
            require(
                independent > 0
                    && usize::from(draft.confidence_inputs.independent_lineages) <= independent,
                "copied/common-owner evidence cannot count as independent confirmation",
            )?;
            let required = [
                "conversation-and-tools",
                "search-planning",
                "market-research",
                "news-analysis",
                "source-comparison",
                "entry-readiness",
                "sector-selection",
                dossier.sector_skill.as_str(),
                dossier.subsector_skill.as_str(),
                "company-profile",
                "relationship-analysis",
                "holding-watchdog",
                "perpetual-review",
            ];
            let mut skills = BTreeSet::new();
            require(
                dossier.sector_skill.starts_with("sector-")
                    && dossier.subsector_skill.starts_with("subsector-")
                    && dossier.skills.len() == required.len(),
                "incomplete skill routing",
            )?;
            for s in &dossier.skills {
                require(
                    skills.insert(&s.name)
                        && required.contains(&s.name.as_str())
                        && account.approved_skills.get(&s.name) == Some(&s.content_hash)
                        && !s.artifact_ids.is_empty()
                        && s.artifact_ids.iter().all(|id| records.contains_key(id)),
                    "skill hash or finalized artifacts are unapproved",
                )?;
            }
            for kind in ["company_profile", "relationships", "watchdog", "economics"] {
                require(
                    dossier.records.iter().any(|r| r.kind == kind),
                    "finalized required research artifact missing",
                )?;
            }
            let evidence = &draft.formula_evidence;
            let base = trady_policy::confidence(&draft.confidence_inputs).total();
            require(
                evidence.base_confidence_score == base
                    && evidence.confidence_score <= base
                    && evidence.last_completed_at <= evidence.available_at
                    && evidence.available_at <= now
                    && now < evidence.valid_until
                    && evidence.atr > Decimal::ZERO
                    && evidence.completed_bars > u32::from(evidence.atr_lookback),
                "formula indicators lack point-in-time evidence",
            )?;
        }
        // Every claimed broker capability and ranking artifact must be separately reviewed.
        for id in p
            .screening_observations
            .broker_support
            .iter()
            .map(|s| &s.evidence_id)
            .chain(
                p.screening_observations
                    .priorities
                    .iter()
                    .flat_map(|p| p.evidence_ids.iter()),
            )
        {
            require(
                i.dependencies.contains(id),
                "screening support is missing original evidence binding",
            )?;
            self.store.resolve(id, now)?;
        }
        Ok(())
    }
    pub(super) fn validate_forecast(
        &self,
        i: &SourceImport,
        f: &ForecastImport,
        now: DateTime<Utc>,
    ) -> Result<()> {
        payload(self.config(), i, now)?;
        let o = &f.observation;
        let mut origins = BTreeMap::new();
        for id in &i.dependencies {
            let dep = self.store.resolve(id, now)?;
            origins
                .entry(dep.origin_lineage_id.clone())
                .or_insert_with(Vec::new)
                .push(dep);
        }
        let mut all = vec![(&o.evidence_lineages, o.available_at)];
        for s in [
            &o.signals.research,
            &o.signals.sentiment,
            &o.signals.quantitative,
        ] {
            require(
                s.observed_at <= s.available_at
                    && s.available_at <= s.generated_at
                    && s.generated_at <= now
                    && now < s.valid_until
                    && fresh(s.observed_at, now, 300)
                    && valid_id(&s.model_version),
                "stale or future signal observation",
            )?;
            all.push((&s.evidence_lineages, s.available_at));
        }
        for (lineages, available) in all {
            require(
                !lineages.is_empty()
                    && lineages.len() <= 64
                    && lineages.iter().collect::<BTreeSet<_>>().len() == lineages.len()
                    && lineages.iter().all(|l| {
                        origins.get(l).is_some_and(|imports| {
                            imports.iter().any(|d| d.available_at <= available)
                        })
                    }),
                "forecast uses unreviewed or unavailable source lineage",
            )?;
        }
        Ok(())
    }
    pub(super) fn validate_model_evidence(
        &self,
        model: &ApprovedModel,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let pin = self
            .config()
            .model
            .as_ref()
            .ok_or_else(|| Error("model deployment unavailable".into()))?;
        model.validate(now)?;
        let expected = [
            (
                "dataset_verification",
                &model.approval.dataset_verification_sha256,
            ),
            (
                "prospective_shadow",
                &model.approval.prospective_shadow_sha256,
            ),
            ("owner_approval", &model.approval.owner_approval_sha256),
        ];
        let mut found = BTreeSet::new();
        for id in &pin.evidence_import_ids {
            let imported = self.store.resolve(id, now)?;
            let ImportPayload::Evidence(e) = &imported.payload else {
                return Err(Error(
                    "model approval reference is not verified evidence".into(),
                ));
            };
            for (kind, hash) in expected {
                if e.record.kind == kind && &imported.raw_document_sha256 == hash {
                    found.insert(kind);
                }
            }
        }
        require(
            found.len() == 3,
            "model lacks resolved dataset, prospective shadow or owner approval evidence",
        )
    }
}

/// Maximum matching counts a corroboration set with distinct owners AND distinct
/// original lineages. Separate counters or greedy dedup can over/undercount when
/// owners report several events and other owners copy one of those events.
fn independent_origins<'a>(origins: impl Iterator<Item = (&'a str, &'a str)>) -> usize {
    let mut graph: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (owner, lineage) in origins {
        graph.entry(owner).or_default().insert(lineage);
    }
    fn augment<'a>(
        owner: &'a str,
        graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
        matched: &mut BTreeMap<&'a str, &'a str>,
        seen: &mut BTreeSet<&'a str>,
    ) -> bool {
        for lineage in &graph[owner] {
            if !seen.insert(*lineage) {
                continue;
            }
            let previous = matched.get(lineage).copied();
            if previous.is_none_or(|other| augment(other, graph, matched, seen)) {
                matched.insert(*lineage, owner);
                return true;
            }
        }
        false
    }
    let mut matched = BTreeMap::new();
    for owner in graph.keys() {
        augment(owner, &graph, &mut matched, &mut BTreeSet::new());
    }
    matched.len()
}
