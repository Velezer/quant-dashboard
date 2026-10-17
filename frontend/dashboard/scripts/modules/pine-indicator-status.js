/** Owns Pine indicator runtime status and its user-visible diagnostics list. */
export function createPineIndicatorStatus({ status, diagnosticList }) {
    function setStatus(message, state = 'waiting') {
        if (!status) return;
        status.textContent = message;
        status.dataset.state = state;
        status.className = `mt-2 text-[10px] ${state === 'error' ? 'text-rose-300' : state === 'ready' ? 'text-emerald-300' : state === 'loading' ? 'text-amber-300' : 'text-gray-400'}`;
    }

    function showDiagnostics(diagnostics = [], approximations = []) {
        if (!diagnosticList) return;
        const items = [...diagnostics, ...approximations.map(message => ({ severity: 'approximation', message }))];
        if (!items.length) {
            diagnosticList.textContent = 'No parse errors. Runtime support is bounded and unsupported features will stop evaluation.';
            return;
        }
        const list = document.createElement('ul');
        list.className = 'space-y-1';
        for (const item of items) {
            const row = document.createElement('li');
            row.textContent = `${item.severity === 'error' ? 'Unsupported/error' : 'Known approximation'}${item.line ? ` · ${item.line}:${item.column}` : ''}: ${item.message}`;
            row.className = item.severity === 'error' ? 'text-rose-300' : 'text-amber-200';
            list.append(row);
        }
        diagnosticList.replaceChildren(list);
    }

    return { setStatus, showDiagnostics };
}
