import test from 'node:test';
import assert from 'node:assert/strict';
import { renderApplicationBuild } from './app-build-version.js';

function fakeElement() {
    return {
        dataset: {},
        attributes: {},
        setAttribute(name, value) { this.attributes[name] = value; },
    };
}

test('shows version and a verifiable build fingerprint', () => {
    const element = fakeElement();
    const buildId = '0123456789abcdef';
    renderApplicationBuild({ version: '0.2.0', build_id: buildId }, element);
    assert.equal(element.textContent, 'v0.2.0 · 0123456789ab');
    assert.match(element.title, new RegExp(buildId));
    assert.equal(element.dataset.buildId, buildId);
    assert.match(element.attributes['aria-label'], new RegExp(buildId));
});

test('reports unavailable build metadata instead of implying it is current', () => {
    const element = fakeElement();
    renderApplicationBuild({ version: '0.2.0' }, element);
    assert.equal(element.textContent, 'BUILD UNAVAILABLE');
    assert.equal(element.dataset.buildId, undefined);
});
