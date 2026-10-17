export const MAX_ACTIVE_PINE_INDICATORS = 2;
import { NWE_MAX_OUTSIDE_DISTANCE_ID } from './pine-sources.js';

export function pineChartPlots(outputs, timeframe, candleCount) {
    const currentOutputs = (Array.isArray(outputs) ? outputs : [])
        .filter(output => output?.timeframe === timeframe);
    const plotsFor = output => (Array.isArray(output?.plots) ? output.plots : [])
        .filter(plot => Array.isArray(plot.values) && plot.values.length === candleCount);
    const overlayPlots = currentOutputs.flatMap(plotsFor).filter(plot => plot.overlay);
    const panePlots = currentOutputs.flatMap(plotsFor).filter(plot => !plot.overlay);
    const dssOutput = currentOutputs.find(output => {
        if (output.name !== 'DSS Bressert (Double Smoothed Stochastic)') return false;
        const titles = new Set(plotsFor(output).filter(plot => !plot.overlay).map(plot => plot.title));
        return titles.has('DSS') && titles.has('Trigger');
    }) || null;
    const nweOutsideDistanceOutput = currentOutputs.find(output =>
        String(output.id) === NWE_MAX_OUTSIDE_DISTANCE_ID
        && plotsFor(output).some(plot => plot.title === 'Max above upper band (%)')
        && plotsFor(output).some(plot => plot.title === 'Max below lower band (%)')) || null;
    const paneOutput = nweOutsideDistanceOutput || dssOutput;
    return {
        overlayPlots,
        panePlots,
        hasDssBressert: dssOutput !== null,
        dssOutput: dssOutput ? { ...dssOutput, plots: plotsFor(dssOutput).filter(plot => !plot.overlay) } : null,
        nweOutsideDistanceOutput: nweOutsideDistanceOutput
            ? { ...nweOutsideDistanceOutput, plots: plotsFor(nweOutsideDistanceOutput).filter(plot => !plot.overlay) }
            : null,
        paneOutput: paneOutput ? { ...paneOutput, plots: plotsFor(paneOutput).filter(plot => !plot.overlay) } : null,
    };
}
