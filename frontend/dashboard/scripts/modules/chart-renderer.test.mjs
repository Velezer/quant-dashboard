import test from 'node:test';
import assert from 'node:assert/strict';
import { withChartPricePaneClip } from './chart-renderer.js';

test('price pane overlay drawing is clipped to the plot bounds', () => {
    const operations = [];
    const context = {
        save() { operations.push('save'); },
        beginPath() { operations.push('beginPath'); },
        rect(...bounds) { operations.push(['rect', ...bounds]); },
        clip() { operations.push('clip'); },
        restore() { operations.push('restore'); },
    };

    withChartPricePaneClip(
        context,
        { left: 15, top: 20, width: 889, height: 240 },
        () => operations.push('draw overlay'),
    );

    assert.deepEqual(operations, [
        'save',
        'beginPath',
        ['rect', 15, 20, 889, 240],
        'clip',
        'draw overlay',
        'restore',
    ]);
});

test('price pane clipping restores the canvas if overlay drawing throws', () => {
    const operations = [];
    const context = {
        save() { operations.push('save'); },
        beginPath() { operations.push('beginPath'); },
        rect() { operations.push('rect'); },
        clip() { operations.push('clip'); },
        restore() { operations.push('restore'); },
    };

    assert.throws(
        () => withChartPricePaneClip(context, { left: 0, top: 0, width: 10, height: 10 }, () => {
            throw new Error('overlay failed');
        }),
        /overlay failed/,
    );
    assert.equal(operations.at(-1), 'restore');
});
