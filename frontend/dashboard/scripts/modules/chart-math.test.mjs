import test from 'node:test';
import assert from 'node:assert/strict';
import { QuantChart } from './chart-math.js';

test('price scale uses stable rounded bounds and readable intervals', () => {
    const bounds = QuantChart.priceScale(0.0684, 0.1348, 5);
    const scale = QuantChart.priceScale(bounds.min, bounds.max, 5);

    assert.equal(bounds.min, 0.06);
    assert.equal(bounds.max, 0.14);
    assert.equal(scale.step, 0.02);
    assert.deepEqual(scale.ticks, [0.06, 0.08, 0.1, 0.12, 0.14]);
});

test('price scale retains enough precision for small-valued instruments', () => {
    const scale = QuantChart.priceScale(0.000123, 0.000287, 5);

    assert.ok(scale.step > 0 && scale.step < 0.0001);
    assert.ok(scale.min <= 0.000123);
    assert.ok(scale.max >= 0.000287);
    assert.ok(scale.ticks.every(Number.isFinite));
});

test('invalid price ranges return no ticks', () => {
    assert.deepEqual(QuantChart.priceScale(2, 2), { min: 2, max: 2, step: 0, ticks: [] });
});

test('indicator scale zoom stays anchored at the pointer value', () => {
    assert.deepEqual(QuantChart.zoomValueScale(0, 100, 25, 2), { min: 12.5, max: 62.5 });
    assert.deepEqual(QuantChart.zoomValueScale(0, 100, 25, 1), { min: 0, max: 100 });
});

test('indicator scale zoom remains bounded and maps axis pixels to values', () => {
    const zoomedIn = QuantChart.zoomValueScale(0, 100, 50, 100);
    const zoomedOut = QuantChart.zoomValueScale(0, 100, 50, 0.01);

    assert.equal(zoomedIn.max - zoomedIn.min, 2);
    assert.equal(zoomedOut.max - zoomedOut.min, 200);
    assert.equal(QuantChart.valueAtScaleY(12, 12, 100, 0, 100), 100);
    assert.equal(QuantChart.valueAtScaleY(112, 12, 100, 0, 100), 0);
    assert.equal(QuantChart.valueAtScaleY(62, 12, 100, 0, 100), 50);
});

test('indicator scale pan keeps its zoom span while moving the visible range', () => {
    const panUp = QuantChart.panValueScale(20, 40, -5);
    const panDown = QuantChart.panValueScale(20, 40, 7.5);

    assert.deepEqual(panUp, { min: 15, max: 35 });
    assert.deepEqual(panDown, { min: 27.5, max: 47.5 });
    assert.equal(panUp.max - panUp.min, 20);
    assert.deepEqual(QuantChart.panValueScale(0, 100, Number.NaN), { min: 0, max: 100 });
});
