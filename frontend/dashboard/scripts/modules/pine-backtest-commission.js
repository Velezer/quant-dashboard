export function pineBacktestCommissionLabel(fees) {
    const model = typeof fees?.model === 'string' && fees.model.trim() ? fees.model.trim() : 'Commission model unavailable';
    const basis = typeof fees?.return_basis === 'string' && fees.return_basis.trim() ? fees.return_basis.trim() : 'return basis unavailable';
    return `Commission: ${model} · ${basis}`;
}
