const dashboardBindings = new Map();

/**
 * Shares only the named state and functions needed across the static dashboard
 * module graph. The getters keep each feature's mutable state in its owner.
 */
export function publishDashboardBindings(owner, definitions) {
    for (const [name, definition] of Object.entries(definitions)) {
        const previous = dashboardBindings.get(name);
        if (previous && previous.owner !== owner) {
            throw new Error(`Dashboard binding ${name} is already owned by ${previous.owner}.`);
        }
        if (typeof definition?.get !== 'function') {
            throw new TypeError(`Dashboard binding ${name} must provide a getter.`);
        }

        const binding = { owner, ...definition };
        dashboardBindings.set(name, binding);
        const descriptor = {
            configurable: true,
            enumerable: false,
            get: binding.get,
        };
        if (typeof binding.set === 'function') descriptor.set = binding.set;
        Object.defineProperty(globalThis, name, descriptor);
    }
}

export function getDashboardBinding(name) {
    return dashboardBindings.get(name)?.get();
}
