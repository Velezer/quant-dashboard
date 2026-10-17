function inputsAreValid(inputSet) {
    return Object.values(inputSet).every(input => {
        const valid = input.checkValidity();
        input.setAttribute('aria-invalid', String(!valid));
        input.classList.toggle('border-rose-600', !valid);
        return valid;
    });
}

export function parseRuleInputs(fields) {
    const rule = {
        risk_pct: Number(fields.risk_pct.value),
        reward_pct: Number(fields.reward_pct.value),
        win_rate_pct: Number(fields.win_rate_pct.value),
        max_trades_per_day: Number(fields.max_trades_per_day.value),
    };
    if (!inputsAreValid(fields) || !Object.values(rule).every(Number.isFinite)) return null;
    return rule;
}

export function parseGrowthProjectionInputs(fields) {
    const projection = {
        starting_balance_units: Number(fields.starting_balance_units.value),
        trade_size_pct_of_equity: Number(fields.trade_size_pct_of_equity.value),
        trading_days: Number(fields.trading_days.value),
    };
    if (!inputsAreValid(fields)
        || !Object.values(projection).every(Number.isFinite)
        || !Number.isInteger(projection.trading_days)) return null;
    return projection;
}
