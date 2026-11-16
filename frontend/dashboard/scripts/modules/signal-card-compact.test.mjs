import test from 'node:test';
import assert from 'node:assert/strict';
import {createSignalScreenerCard} from './signal-screener-card.js';
import {signalDisplayScore, signalScoreState} from './signal-screener-model.js';
const render = createSignalScreenerCard({escape:String, appLabel:()=> 'Pintu',
    scoreModelFor:()=>({score_scale_max:60}), signalScoreState,
    signalScorePresentation:()=>({scoreScaleMax:60}), signalDisplayScore,
    signalScoreWeightSummary:()=> 'Equal weights', signalPriceBarAgeLabel:()=> '2m ago',
    formatPrice:String, ageLabel:()=> '2m ago', renderScoreBasis:()=> '<div>Six indicator breakdown</div>'});
const signal = {signal_id:'apt-30m',symbol:'APT',timeframe:'30m',direction:'SHORT',
    exchange_id:'pintu',product_id:'pintu_pro_futures',signal_strategy_name:'Signed model',
    score:-12,setup_strength:12,score_contract:'signed_indicator_sum_v1',current_price:3,
    price_is_fresh:true,score_basis:{score_scale_max:60}};
test('compact card keeps primary facts and puts diagnostics in a separate closed disclosure',()=>{
    const html = render(signal,1,false);
    const button = html.match(/<button[\s\S]*?<\/button>/)[0];
    assert.match(button,/APT/);
    assert.match(button,/30M/);
    assert.match(button,/SHORT/);
    assert.match(button,/−12\.0/);
    assert.doesNotMatch(button,/Six indicator breakdown|cancellation|profile cutoff/);
    assert.match(html,/<\/button>\s*<details class="signal-card-details">/);
    assert.match(html,/<summary[^>]*>Details<\/summary>/);
    assert.match(html,/Six indicator breakdown/);
});
test('stale/no-entry warning remains visible without opening details',()=>{
    const button = render({...signal,price_is_fresh:false},1,true).match(/<button[\s\S]*?<\/button>/)[0];
    assert.match(button,/Stale · no market entry/);
    assert.match(button,/aria-pressed="true"/);
});
