export function pineBacktestDiagnostics(diagnostics) {
    if (!Array.isArray(diagnostics)) {
        return { state: 'unavailable', text: 'Runtime diagnostics are unavailable in this response.' };
    }
    const messages = diagnostics.map(item => {
        const message = typeof item?.message === 'string' ? item.message.trim() : '';
        if (!message) return '';
        const severity = typeof item?.severity === 'string' ? item.severity.trim() : '';
        return severity ? `${severity}: ${message}` : message;
    }).filter(Boolean);
    return messages.length
        ? { state: 'ready', text: messages.join('\n\n') }
        : { state: 'ready', text: 'No additional runtime diagnostics were reported for this run.' };
}

export function renderPineBacktestDiagnostics(target, diagnostics) {
    if (!target) return;
    const summary = diagnostics === null
        ? { state: 'empty', text: 'Runtime diagnostics appear after a successful backtest.' }
        : pineBacktestDiagnostics(diagnostics);
    target.textContent = summary.text;
    target.dataset.state = summary.state;
}
