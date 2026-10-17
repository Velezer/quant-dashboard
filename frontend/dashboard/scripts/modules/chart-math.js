export const QuantChart = (() => {
    const MIN_INDICATOR_SCALE_SPAN = 2;
    const MAX_INDICATOR_SCALE_SPAN = 200;
    const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
    const number = value => value === null || value === undefined || value === ''
        ? null
        : (Number.isFinite(Number(value)) ? Number(value) : null);
    function viewport(total, size, offset) {
        const span = clamp(Number(size) || 45, Math.min(10, total), total);
        const pan = clamp(Number(offset) || 0, 0, Math.max(0, total - span));
        return { size: span, offset: pan, left: total - pan - span, right: total - pan };
    }
    function priceScale(minimum, maximum, targetIntervals = 5) {
        const min = number(minimum);
        const max = number(maximum);
        if (min === null || max === null || max <= min) return { min, max, step: 0, ticks: [] };

        const intervals = Math.max(2, Math.floor(Number(targetIntervals) || 5));
        const rawStep = (max - min) / intervals;
        const magnitude = 10 ** Math.floor(Math.log10(rawStep));
        const fraction = rawStep / magnitude;
        const stepFactor = fraction < 1.5 ? 1 : fraction < 3.5 ? 2 : fraction < 7.5 ? 5 : 10;
        const step = stepFactor * magnitude;
        const snappedMin = Math.floor(min / step + 1e-10) * step;
        const snappedMax = Math.ceil(max / step - 1e-10) * step;
        const scaleMin = min > 0 && snappedMin <= 0 ? min : snappedMin;
        const scaleMax = snappedMax > scaleMin ? snappedMax : max;
        const ticks = [];
        let tick = Math.ceil(scaleMin / step - 1e-10) * step;
        const tickLimit = intervals + 3;
        while (tick <= scaleMax + step * 1e-8 && ticks.length < tickLimit) {
            ticks.push(Number(tick.toPrecision(12)));
            tick += step;
        }
        return { min: scaleMin, max: scaleMax, step, ticks };
    }
    function zoomValueScale(minimum, maximum, anchor, factor) {
        const min = number(minimum);
        const max = number(maximum);
        const point = number(anchor);
        const zoomFactor = number(factor);
        if (min === null || max === null || max <= min || point === null || zoomFactor === null || zoomFactor <= 0) {
            return { min, max };
        }
        const currentSpan = max - min;
        const nextSpan = clamp(currentSpan / zoomFactor, MIN_INDICATOR_SCALE_SPAN, MAX_INDICATOR_SCALE_SPAN);
        const anchorFraction = clamp((point - min) / currentSpan, 0, 1);
        return {
            min: point - anchorFraction * nextSpan,
            max: point + (1 - anchorFraction) * nextSpan,
        };
    }
    function panValueScale(minimum, maximum, delta) {
        const min = number(minimum);
        const max = number(maximum);
        const shift = number(delta);
        if (min === null || max === null || max <= min || shift === null) return { min, max };
        return { min: min + shift, max: max + shift };
    }
    function valueAtScaleY(y, top, height, minimum, maximum) {
        const pointY = number(y);
        const plotTop = number(top);
        const plotHeight = number(height);
        const min = number(minimum);
        const max = number(maximum);
        if (pointY === null || plotTop === null || plotHeight === null || plotHeight <= 0 || min === null || max === null || max <= min) return null;
        return max - (pointY - plotTop) / plotHeight * (max - min);
    }
    function zoom(total, size, offset, nextSize, anchor = 0.5) {
        const before = viewport(total, size, offset);
        const fraction = clamp(anchor, 0, 1);
        const next = viewport(total, nextSize, offset);
        const anchoredBar = before.left + before.size * fraction;
        return viewport(total, next.size, total - anchoredBar - next.size * (1 - fraction));
    }
    function key(signal) {
        if (!signal) return '';
        return signal.signal_id || [
            signal.exchange_id || signal.exchange || '',
            signal.product_id || '',
            signal.asset_category_id || '',
            signal.asset || signal.symbol || '',
            signal.signal_strategy_id || signal.strategy_id || signal.strategy || '',
            signal.timeframe || '',
            signal.direction || '',
        ].join('|');
    }
    return { clamp, number, viewport, priceScale, zoomValueScale, panValueScale, valueAtScaleY, zoom, key };
})();
