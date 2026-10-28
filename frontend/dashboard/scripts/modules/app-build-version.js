/** Displays the server build identity shared by every exchange view. */
export function renderApplicationBuild(status, element = document.getElementById('app-build-version')) {
    if (!element) return;
    const version = typeof status?.version === 'string' ? status.version.trim() : '';
    const buildId = typeof status?.build_id === 'string' ? status.build_id.trim() : '';
    if (!version || !buildId) {
        element.textContent = 'BUILD UNAVAILABLE';
        element.title = 'Could not verify the deployed application build. Runtime status did not include a version and source fingerprint.';
        element.setAttribute('aria-label', 'Application build identifier unavailable');
        delete element.dataset.buildId;
        return;
    }
    element.textContent = `v${version} · ${buildId.slice(0, 12)}`;
    element.title = `Version ${version} · source fingerprint ${buildId} (not a Git commit). This build serves all exchange views.`;
    element.setAttribute('aria-label', `Application version ${version}, build ${buildId}`);
    element.dataset.buildId = buildId;
}
