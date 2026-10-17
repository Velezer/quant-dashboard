const POLL_INTERVAL_MS = 750;
const TERMINAL_STATES = new Set(['completed', 'failed', 'cancelled']);

export function setPineBacktestStatus(status, message, state = 'waiting') {
    status.textContent = message;
    status.dataset.state = state;
    status.className = `mt-2 text-[10px] ${state === 'error' ? 'text-rose-300' : state === 'ready' ? 'text-emerald-300' : state === 'loading' ? 'text-amber-300' : 'text-gray-400'}`;
}

export function displayCandleTimeUtc(value) {
    const timestamp = value == null ? NaN : Number(value);
    if (!Number.isFinite(timestamp)) return '—';
    const date = new Date(timestamp / 1_000_000);
    return Number.isFinite(date.getTime()) ? `${date.toISOString().slice(0, 16).replace('T', ' ')} UTC` : '—';
}

export function parseBacktestDateRange(fromDate, toDate) {
    const from = String(fromDate || '').trim();
    const through = String(toDate || '').trim();
    if (Boolean(from) !== Boolean(through)) {
        throw new Error('Choose both UTC dates, or clear both to use the latest selected history depth.');
    }
    if (!from) return null;
    const fromTime = Date.parse(`${from}T00:00:00.000Z`);
    const throughTime = Date.parse(`${through}T00:00:00.000Z`);
    const fromIsCalendarDate = /^\d{4}-\d{2}-\d{2}$/.test(from)
        && Number.isFinite(fromTime)
        && new Date(fromTime).toISOString().slice(0, 10) === from;
    const throughIsCalendarDate = /^\d{4}-\d{2}-\d{2}$/.test(through)
        && Number.isFinite(throughTime)
        && new Date(throughTime).toISOString().slice(0, 10) === through;
    const toTime = throughTime + 86_400_000;
    if (!fromIsCalendarDate || !throughIsCalendarDate || !Number.isFinite(toTime) || fromTime >= toTime) {
        throw new Error('The selected UTC date range is invalid.');
    }
    return { from_time_ms: fromTime, to_time_exclusive_ms: toTime };
}

function jobPath(jobId) {
    return `/api/pine/backtest/jobs/${encodeURIComponent(jobId)}`;
}

function parseJobState(payload) {
    const state = payload?.state;
    if (!['queued', 'running', 'cancelling', ...TERMINAL_STATES].includes(state)) {
        throw new Error('Service returned an invalid backtest job state.');
    }
    const progress = Number(payload.progress_pct);
    return {
        state,
        progress_pct: Number.isFinite(progress) ? Math.min(100, Math.max(0, progress)) : 0,
        phase: typeof payload.phase === 'string' ? payload.phase : '',
        result: payload.result,
        error: typeof payload.error === 'string' ? payload.error : '',
    };
}

function pause() {
    return new Promise(resolve => setTimeout(resolve, POLL_INTERVAL_MS));
}

export function createPineBacktestJobClient(requestJson) {
    let activeJob = null;
    let revision = 0;
    let submitting = false;

    function notify(job, event) {
        job.lastState = event.state || job.lastState;
        job.onState(event);
    }

    function finish(job) {
        if (activeJob !== job) return;
        job.terminal = true;
        job.pollController?.abort();
        activeJob = null;
    }

    async function requestStop(job) {
        if (activeJob !== job || job.cancelPending) return;
        job.cancelPending = true;
        notify(job, { state: 'stopping', progress_pct: job.progress_pct, detached: job.detached, cancel_pending: true });
        try {
            const response = parseJobState(await requestJson(`${jobPath(job.id)}/cancel`, { method: 'POST' }));
            job.cancelPending = false;
            if (activeJob !== job) return;
            if (TERMINAL_STATES.has(response.state)) {
                job.terminalResult = {
                    ...response,
                    detached: job.detached,
                    result_unavailable: response.state === 'completed' && !response.result,
                };
                finish(job);
                notify(job, job.terminalResult);
                return;
            }
            const shouldResumePolling = job.pollingPaused && !job.detached;
            job.progress_pct = response.progress_pct;
            job.cancelRequested = true;
            if (shouldResumePolling) job.pollingPaused = false;
            notify(job, {
                ...response,
                detached: job.detached,
                cancel_requested: true,
            });
            if (shouldResumePolling) void poll(job);
        } catch (error) {
            job.cancelPending = false;
            if (activeJob === job) notify(job, {
                state: 'stop_unconfirmed',
                progress_pct: job.progress_pct,
                detached: job.detached,
                error: error.message,
            });
        }
    }

    async function poll(job) {
        while (activeJob === job && !job.detached && job.revision === revision) {
            const pollController = new AbortController();
            job.pollController = pollController;
            let response;
            try {
                response = parseJobState(await requestJson(jobPath(job.id), { signal: pollController.signal }));
            } catch (error) {
                if (error?.name === 'AbortError' && (job.detached || job.revision !== revision || job.terminal)) return job.terminalResult || { state: 'detached' };
                job.pollingPaused = true;
                notify(job, { state: 'status_unavailable', detached: false, error: error.message, progress_pct: job.progress_pct });
                return { state: 'status_unavailable' };
            } finally {
                if (job.pollController === pollController) job.pollController = null;
            }
            if (activeJob !== job || job.detached || job.revision !== revision) return { state: 'detached' };
            job.progress_pct = response.progress_pct;
            notify(job, job.cancelRequested ? { ...response, cancel_requested: true } : response);
            if (response.state === 'completed') {
                if (!response.result || typeof response.result !== 'object') {
                    finish(job);
                    const invalid = { state: 'failed', error: 'Completed backtest result was missing or invalid.' };
                    job.terminalResult = invalid;
                    notify(job, invalid);
                    return invalid;
                }
                job.terminalResult = response;
                finish(job);
                return response;
            }
            if (response.state === 'failed' || response.state === 'cancelled') {
                job.terminalResult = response;
                finish(job);
                return response;
            }
            await pause();
        }
        return job.terminalResult || { state: 'detached' };
    }

    return {
        get busy() { return submitting || activeJob !== null; },
        get submitting() { return submitting; },
        get activeJob() {
            return activeJob && {
                detached: activeJob.detached,
                pollingPaused: activeJob.pollingPaused,
                cancelPending: activeJob.cancelPending,
                state: activeJob.lastState,
            };
        },
        async start(request, onState) {
            if (submitting || activeJob) throw new Error('A Pine backtest job is already active.');
            const startRevision = revision;
            submitting = true;
            try {
                const response = await requestJson('/api/pine/backtest', {
                    method: 'POST',
                    body: JSON.stringify(request),
                });
                const jobId = response?.job_id;
                if (typeof jobId !== 'string' && typeof jobId !== 'number' || String(jobId).trim() === '') {
                    throw new Error('Service did not return a valid Pine backtest job ID.');
                }
                const job = {
                    id: String(jobId),
                    onState,
                    revision: startRevision,
                    progress_pct: 0,
                    detached: startRevision !== revision,
                    cancelPending: false,
                    cancelRequested: false,
                };
                activeJob = job;
                if (job.detached) {
                    notify(job, { state: 'stopping', detached: true, progress_pct: 0, cancel_pending: true });
                    void requestStop(job);
                    return { state: 'detached' };
                }
                notify(job, { state: 'queued', progress_pct: 0 });
                return await poll(job);
            } finally {
                submitting = false;
            }
        },
        cancel() {
            if (activeJob) return requestStop(activeJob);
        },
        invalidate() {
            revision += 1;
            const job = activeJob;
            if (!job) return;
            job.detached = true;
            job.pollController?.abort();
            notify(job, { state: 'stopping', detached: true, progress_pct: job.progress_pct, cancel_pending: true });
            void requestStop(job);
        },
    };
}

export function createPineBacktestJobUi({ runButton, cancelButton, jobClient, canRun, setStatus, clearResults, currentRevision }) {
    const container = document.getElementById('pineStrategyBacktestProgressContainer');
    const progress = document.getElementById('pineStrategyBacktestProgress');
    const label = document.getElementById('pineStrategyBacktestProgressLabel');

    function updateControls() {
        const job = jobClient.activeJob;
        runButton.disabled = !canRun() || jobClient.busy;
        if (cancelButton) {
            cancelButton.hidden = !job;
            cancelButton.disabled = !job || job.cancelPending;
            cancelButton.textContent = job?.detached
                ? 'Request stop again'
                : job?.pollingPaused
                    ? 'Request stop and retry status'
                    : job?.state === 'stop_unconfirmed' ? 'Retry stop request' : 'Cancel backtest';
        }
    }

    function resetProgress() {
        if (container) container.hidden = true;
        if (progress) progress.value = 0;
        if (label) label.textContent = 'Waiting for a backtest run.';
    }

    function showProgress(event, detached = false) {
        if (container) container.hidden = detached;
        if (progress) {
            progress.max = 100;
            progress.value = Number.isFinite(event.progress_pct) ? event.progress_pct : 0;
            progress.setAttribute('aria-valuetext', `${Math.round(progress.value)}%`);
        }
        if (label) label.textContent = event.state === 'queued'
            ? `${event.phase || 'Queued'} · waiting for the backtest worker.`
            : event.state === 'running'
            ? `${event.phase || 'Running'} · ${Math.round(progress?.value || 0)}%${event.cancel_requested ? ' · stop requested' : ''}`
            : event.state === 'stopping'
            ? detached ? 'Stop requested · prior run is no longer being followed.' : 'Cancellation requested · waiting for the job state.'
            : event.state === 'completed'
            ? event.result_unavailable ? 'Completed · result not retrieved after stop request.' : 'Completed · 100%'
            : event.state === 'cancelled'
            ? 'Cancelled · no result was applied.'
            : event.state === 'failed'
            ? 'Failed · inspect the message below and retry after recovery.'
            : event.state === 'status_unavailable'
            ? 'Status unavailable · request a stop or retry the status check.'
            : event.state === 'stop_unconfirmed'
            ? 'Stop was not confirmed · request it again or retry after service recovery.'
            : `${event.state || 'Waiting'}${Number.isFinite(event.progress_pct) ? ` · ${Math.round(event.progress_pct)}%` : ''}`;
    }

    function render(event, runRevision, strategyName) {
        updateControls();
        const detached = event.detached === true || runRevision !== currentRevision();
        showProgress(event, detached);
        if (detached) {
            if (event.state === 'cancelled') setStatus('The previous backtest was cancelled after its settings changed. Its results were discarded; run the current selection to see a fresh result.', 'waiting');
            else if (event.state === 'completed' || event.state === 'failed') setStatus(`The previous backtest ${event.state} after its settings changed. Its result was discarded; run the current selection to see a fresh result.`, event.state === 'failed' ? 'error' : 'waiting');
            else if (event.state === 'stop_unconfirmed') setStatus(`Could not confirm a stop for the previous backtest: ${event.error || 'service unavailable'}. The job may still be running. Use “Request stop again” after the service recovers.`, 'error');
            else if (event.cancel_requested || event.state === 'stopping') setStatus('Settings changed. A stop was requested for the previous backtest; its results will not be shown for this selection. Check or request stop again before starting another run.', 'loading');
            updateControls();
            return;
        }
        if (event.state === 'stopping' || event.cancel_requested) setStatus(`Cancellation requested for ${strategyName || 'Pine strategy'} backtest. Waiting for the job to confirm its state.`, 'loading');
        else if (event.state === 'queued' || event.state === 'running') setStatus(`${strategyName || 'Pine strategy'} backtest ${event.phase || event.state} · ${Math.round(event.progress_pct || 0)}%. This is transient research output; no orders are placed.`, 'loading');
        else if (event.state === 'cancelled') {
            clearResults();
            showProgress(event);
            setStatus('Backtest cancelled. No result was applied. Adjust the selection or run it again when ready.', 'waiting');
        } else if (event.state === 'failed') {
            clearResults();
            showProgress(event);
            setStatus(`Backtest failed: ${event.error || 'the service reported a job failure'}. Retry after the local service or selected SQLite history recovers.`, 'error');
        } else if (event.state === 'status_unavailable') setStatus(`Backtest status unavailable: ${event.error || 'the service did not respond'}. The job may still be running. Use “Request stop and retry status” to ask the service to stop it and resume status polling.`, 'error');
        else if (event.state === 'stop_unconfirmed') setStatus(`Could not confirm the stop request: ${event.error || 'the service did not respond'}. The backtest may still be running; request stop again after recovery.`, 'error');
        else if (event.state === 'completed' && event.result_unavailable) setStatus('The backtest completed before its stop request took effect, so its result was not retrieved. Run it again to show the result.', 'waiting');
        else if (event.state === 'completed') {
            showProgress({ ...event, progress_pct: 100 });
            setStatus(`${strategyName || 'Pine strategy'} backtest completed. Preparing its transient research results…`, 'loading');
        }
        updateControls();
    }

    function invalidate(message) {
        const hadActiveJob = Boolean(jobClient.activeJob);
        const wasSubmitting = jobClient.submitting;
        jobClient.invalidate();
        clearResults();
        const recovery = hadActiveJob
            ? ' A stop request is being sent for the previous run; its result will not be applied to this selection.'
            : wasSubmitting ? ' Settings changed during submission; an accepted job will be sent a stop request as soon as its ID arrives.' : '';
        setStatus(`${message}${recovery}`, 'waiting');
        updateControls();
    }

    return { updateControls, resetProgress, render, invalidate };
}
