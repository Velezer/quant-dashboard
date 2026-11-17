import test from 'node:test';
import assert from 'node:assert/strict';
import {retainStockbitScoreReference, signalScoreState} from './signal-screener-model.js';
test('retained Stockbit scores remain visible as stale references; legacy rows do not',()=>{
    const row={exchange_id:'stockbit',persistent_score:true,score_status:'stale',score_basis:{score_model_status:'current'}};
    assert.equal(retainStockbitScoreReference(row),true);
    assert.equal(signalScoreState(row).current,false);
    assert.equal(retainStockbitScoreReference({...row,score_basis:{score_model_status:'legacy'}}),false);
    assert.equal(retainStockbitScoreReference({...row,exchange_id:'pintu'}),false);
});
