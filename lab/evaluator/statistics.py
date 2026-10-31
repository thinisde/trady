"""Paired chronological block resampling and the frozen application objective."""
import math
import random
import statistics
from fractions import Fraction
from .common import finite, integer, number, numeric_margin, ratio, require


def mean(values):
    require(len(values) > 0, "empty statistic")
    return finite(math.fsum(values) / len(values))


def drawdown(returns):
    wealth = peak = 1.0
    worst = 0.0
    for value in returns:
        require(value > -1 and math.isfinite(value), "undefined account return")
        wealth = finite(wealth * (1 + value))
        peak = max(peak, wealth)
        worst = max(worst, 1 - wealth / peak)
    return worst


def objective(folds, weights, complexity):
    means = sorted(mean([math.log1p(r) for r in fold]) for fold in folds)
    require(len(means) >= 2, "at least two outer folds required")
    iqr = means[3 * (len(means) - 1) // 4] - means[(len(means) - 1) // 4]
    return finite(statistics.median(means) - float(number(weights["drawdown_weight"])) * max(drawdown(f) for f in folds)
                  - float(number(weights["instability_weight"])) * iqr
                  - float(number(weights["complexity_weight"])) * complexity)


def deterministic_risk(returns, exposures, turnovers):
    """Observed financial risk from exact cash-flow-adjusted session returns.

    No float, log transform or resampling is involved in these hard gates.
    Drawdown uses the same continuous wealth convention as the research report.
    """
    require(returns and len(returns) == len(exposures) == len(turnovers), "incomplete risk observations")
    returns = [Fraction(value) for value in returns]
    exposures, turnovers = [Fraction(value) for value in exposures], [Fraction(value) for value in turnovers]
    require(all(value > -1 for value in returns) and all(0 <= value <= 1 for value in exposures)
            and all(value >= 0 for value in turnovers), "invalid exact risk observations")
    wealth = peak = Fraction(1)
    maximum_drawdown = Fraction(0)
    for value in returns:
        wealth *= 1 + value
        peak = max(peak, wealth)
        maximum_drawdown = max(maximum_drawdown, (peak - wealth) / peak)
    tail_n = (len(returns) + 19) // 20
    return {"net_return": wealth - 1, "max_drawdown": maximum_drawdown,
            "cvar_95_loss": max(Fraction(0), -ratio(sum(sorted(returns)[:tail_n]), tail_n)),
            "worst_session_loss": max(Fraction(0), -min(returns)),
            "mean_turnover": ratio(sum(turnovers), len(turnovers)),
            "mean_exposure": ratio(sum(exposures), len(exposures)), "maximum_exposure": max(exposures)}


def risk_metrics(returns, exposures, turnovers, annual_sessions, deterministic=None):
    exact = deterministic if deterministic is not None else deterministic_risk(returns, exposures, turnovers)
    returns = [float(value) for value in returns]
    n = len(returns)
    standard = statistics.stdev(returns) if n > 1 else 0.0
    downside = math.sqrt(mean([min(0.0, r) ** 2 for r in returns]))
    return {**{key: finite(float(value)) for key, value in exact.items()},
            "mean_log_return": mean([math.log1p(r) for r in returns]),
            "sharpe": mean(returns) / standard * math.sqrt(annual_sessions) if standard > 0 else None,
            "sortino": mean(returns) / downside * math.sqrt(annual_sessions) if downside > 0 else None}


def circular_blocks(length, block_length, generator):
    indices = []
    while len(indices) < length:
        start = generator.randrange(length)
        indices.extend((start + j) % length for j in range(block_length))
    return indices[:length]


def paired_bounds(series, folds, weights, complexities, candidate, neighbors, benchmarks, scenarios, policy, trial_count, seed):
    """One-sided basic bootstrap LCBs, with a predeclared Bonferroni family.

    Entire exchange-session blocks, including all models/cost cases, move
    together. Draws never cross outer-fold boundaries. This preserves the
    observed paired dependence, not an assertion of independent future returns.
    """
    block = integer(policy["block_sessions"], 1, 252)
    repetitions = integer(policy["bootstrap_repetitions"], 100, 100_000)
    looks = integer(policy["planned_looks"], 1, 10_000)
    alpha = number(policy["alpha"])
    require(0 < alpha <= Fraction(1, 20), "invalid confidence level")
    margin = float(numeric_margin(policy))
    family = trial_count * looks * len(benchmarks) * len(scenarios) * (len(neighbors) + 1)
    adjusted_alpha = alpha / family
    sufficient = all(len(fold) >= 2 * block for fold in folds) and repetitions * adjusted_alpha >= 5
    observed, draws = {}, {}
    all_candidates = [candidate, *neighbors]
    for strategy in all_candidates:
        for scenario in scenarios:
            for baseline in benchmarks:
                key = (strategy, scenario, baseline)
                observed[key] = objective([[series[strategy, scenario][i] for i in f] for f in folds], weights, complexities[strategy]) - objective(
                    [[series[baseline, scenario][i] for i in f] for f in folds], weights, complexities[baseline])
                draws[key] = []
    generator = random.Random(seed)
    if sufficient:
        for _ in range(repetitions):
            sampled = [[fold[i] for i in circular_blocks(len(fold), block, generator)] for fold in folds]
            scores = {(strategy, scenario): objective([[series[strategy, scenario][i] for i in f] for f in sampled], weights, complexities[strategy])
                      for strategy in [*all_candidates, *benchmarks] for scenario in scenarios}
            for key in draws:
                strategy, scenario, baseline = key
                draws[key].append(scores[strategy, scenario] - scores[baseline, scenario])
    result = {}
    for key, point in observed.items():
        if sufficient:
            errors = sorted(value - point for value in draws[key])
            # Basic rather than naive percentile interval: subtract the upper
            # bootstrap estimation error. Nearest-rank rounding is conservative.
            upper_index = min(len(errors) - 1, math.ceil((1 - adjusted_alpha) * len(errors)) - 1)
            lower = point - errors[upper_index] - margin
        else:
            # Explicitly not estimable. The output gate is false; do not invent a CI.
            lower = None
        result[key] = {"point": point, "lower": lower}
    return result, {"method": "paired-circular-block-basic-bootstrap-bonferroni-v1", "block_sessions": block,
                    "repetitions": repetitions, "search_trials": trial_count, "planned_looks": looks,
                    "comparison_family": family, "adjusted_alpha": float(adjusted_alpha),
                    "sufficient": sufficient, "numeric_margin": margin}
