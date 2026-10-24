import { installInstrumentRegistry } from './instrument-registry.js';
import { getDashboardBinding } from './runtime-context.js';

installInstrumentRegistry();
getDashboardBinding('initializeInstrumentRegistry')?.();
