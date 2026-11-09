/** Keeps the mobile app/product/timeframe controls easy to reach without covering the screener. */
export function installMobileScreenerControls(doc = document, view = window) {
    const dock = doc.getElementById('mobile-quick-filters');
    const toggle = doc.getElementById('mobileQuickFilterToggle');
    const summary = doc.getElementById('mobileQuickFilterSummary');
    const sheet = doc.getElementById('mobile-quick-filter-sheet');
    if (!dock || !toggle || !summary || !sheet) return;

    const mobile = () => view.matchMedia('(max-width: 760px)').matches;
    const activeLabel = selector => dock.querySelector(`${selector}.active, ${selector}[aria-pressed="true"]`)
        ?.textContent.trim().replace(/\s+/g, ' ') || '';

    function updateSummary() {
        const app = activeLabel('.exchange-btn') || 'Choose app';
        const product = activeLabel('.product-btn') || 'All';
        const timeframe = activeLabel('.screener-timeframe-btn') || 'All';
        summary.textContent = `${app} · ${product} · ${timeframe === 'All' ? 'All timeframes' : timeframe}`;
        toggle.setAttribute('aria-label', `Choose app, product, and timeframe. Current: ${summary.textContent}`);
    }

    function setOpen(open) {
        dock.classList.toggle('is-open', open);
        toggle.setAttribute('aria-expanded', String(open));
        if (mobile()) sheet.hidden = !open;
        else sheet.hidden = false;
    }

    toggle.addEventListener('click', () => setOpen(!dock.classList.contains('is-open')));
    dock.addEventListener('click', event => {
        const control = event.target instanceof Element
            ? event.target.closest('.exchange-btn, .product-btn, .screener-timeframe-btn')
            : null;
        if (!control) return;
        view.requestAnimationFrame(() => {
            updateSummary();
            if (mobile() && control.classList.contains('screener-timeframe-btn')) setOpen(false);
        });
    });
    doc.addEventListener('pointerdown', event => {
        if (mobile() && dock.classList.contains('is-open') && !dock.contains(event.target)) setOpen(false);
    });
    doc.addEventListener('keydown', event => {
        if (event.key === 'Escape' && mobile() && dock.classList.contains('is-open')) {
            setOpen(false);
            toggle.focus();
        }
    });
    view.addEventListener('resize', () => setOpen(false));
    updateSummary();
    setOpen(false);
}
