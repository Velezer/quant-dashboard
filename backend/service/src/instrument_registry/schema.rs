use super::catalog::{seed_pintu, seed_pluang};
use super::*;

pub(crate) const PINTU_CONTRACTS_JSON: &str = r#"[{"contract_unit":"0.001 BTC","maximum_position_usdt":"5000000","minimum_lot":"1","price_tick":"0.1","venue_symbol":"BTCUSDT-PERP"},{"contract_unit":"0.001 ETH","maximum_position_usdt":"2500000","minimum_lot":"100","price_tick":"0.01","venue_symbol":"ETHUSDT-PERP"},{"contract_unit":"1 DOGE","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"DOGEUSDT-PERP"},{"contract_unit":"0.1 SOL","maximum_position_usdt":"2500000","minimum_lot":"10","price_tick":"0.01","venue_symbol":"SOLUSDT-PERP"},{"contract_unit":"0.1 RENDER","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"RENDERUSDT-PERP"},{"contract_unit":"1 AVAX","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.001","venue_symbol":"AVAXUSDT-PERP"},{"contract_unit":"1 NEAR","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"NEARUSDT-PERP"},{"contract_unit":"0.1 XRP","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"XRPUSDT-PERP"},{"contract_unit":"1 WLD","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"WLDUSDT-PERP"},{"contract_unit":"0.1 ARB","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"ARBUSDT-PERP"},{"contract_unit":"1 POL","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"POLUSDT-PERP"},{"contract_unit":"0.1 SUI","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"SUIUSDT-PERP"},{"contract_unit":"1 PEPE","maximum_position_usdt":"1000000","minimum_lot":"10000000","price_tick":"0.0000000001","venue_symbol":"PEPEUSDT-PERP"},{"contract_unit":"0.01 BNB","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.01","venue_symbol":"BNBUSDT-PERP"},{"contract_unit":"1 UNI","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.001","venue_symbol":"UNIUSDT-PERP"},{"contract_unit":"1 SHIB","maximum_position_usdt":"1000000","minimum_lot":"10000000","price_tick":"0.000000001","venue_symbol":"SHIBUSDT-PERP"},{"contract_unit":"1 TROY","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.000001","venue_symbol":"TROYUSDT-PERP"},{"contract_unit":"1 MASK","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"MASKUSDT-PERP"},{"contract_unit":"0.1 AAVE","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.01","venue_symbol":"AAVEUSDT-PERP"},{"contract_unit":"1 APE","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"APEUSDT-PERP"},{"contract_unit":"0.1 ORDI","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"ORDIUSDT-PERP"},{"contract_unit":"1 PEOPLE","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"PEOPLEUSDT-PERP"},{"contract_unit":"0.001 BCH","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.01","venue_symbol":"BCHUSDT-PERP"},{"contract_unit":"1 FTM","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"FTMUSDT-PERP"},{"contract_unit":"0.01 LINK","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"LINKUSDT-PERP"},{"contract_unit":"1 FET","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"FETUSDT-PERP"},{"contract_unit":"0.1 APT","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"APTUSDT-PERP"},{"contract_unit":"1 ARKM","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"ARKMUSDT-PERP"},{"contract_unit":"1 RARE","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"RAREUSDT-PERP"},{"contract_unit":"1 ADA","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"ADAUSDT-PERP"},{"contract_unit":"1 FLOKI","maximum_position_usdt":"1000000","minimum_lot":"1000000","price_tick":"0.00000001","venue_symbol":"FLOKIUSDT-PERP"},{"contract_unit":"1 LDO","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"LDOUSDT-PERP"},{"contract_unit":"0.1 INJ","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"INJUSDT-PERP"},{"contract_unit":"0.001 LTC","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.01","venue_symbol":"LTCUSDT-PERP"},{"contract_unit":"1 SEI","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"SEIUSDT-PERP"},{"contract_unit":"1 RDNT","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"RDNTUSDT-PERP"},{"contract_unit":"0.001 MKR","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.1","venue_symbol":"MKRUSDT-PERP"},{"contract_unit":"0.1 DOT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"DOTUSDT-PERP"},{"contract_unit":"0.1 OP","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"OPUSDT-PERP"},{"contract_unit":"1 RUNE","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"RUNEUSDT-PERP"},{"contract_unit":"0.1 FIL","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"FILUSDT-PERP"},{"contract_unit":"1 PENDLE","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"PENDLEUSDT-PERP"},{"contract_unit":"0.1 DYDX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"DYDXUSDT-PERP"},{"contract_unit":"1 STX","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"STXUSDT-PERP"},{"contract_unit":"1 GALA","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"GALAUSDT-PERP"},{"contract_unit":"1 TRX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"TRXUSDT-PERP"},{"contract_unit":"0.1 SXP","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"SXPUSDT-PERP"},{"contract_unit":"1 ARK","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"ARKUSDT-PERP"},{"contract_unit":"0.01 ETC","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"ETCUSDT-PERP"},{"contract_unit":"0.1 SANTOS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"SANTOSUSDT-PERP"},{"contract_unit":"0.1 EOS","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.001","venue_symbol":"EOSUSDT-PERP"},{"contract_unit":"0.01 ATOM","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.001","venue_symbol":"ATOMUSDT-PERP"},{"contract_unit":"0.1 CRV","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.001","venue_symbol":"CRVUSDT-PERP"},{"contract_unit":"0.1 AR","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"ARUSDT-PERP"},{"contract_unit":"1 ICP","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.001","venue_symbol":"ICPUSDT-PERP"},{"contract_unit":"1 BICO","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"BICOUSDT-PERP"},{"contract_unit":"1 JASMY","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"JASMYUSDT-PERP"},{"contract_unit":"1 OGN","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"OGNUSDT-PERP"},{"contract_unit":"1 IMX","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"IMXUSDT-PERP"},{"contract_unit":"1 DIA","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"DIAUSDT-PERP"},{"contract_unit":"1 SUSHI","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"SUSHIUSDT-PERP"},{"contract_unit":"1 AGLD","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"AGLDUSDT-PERP"},{"contract_unit":"0.1 ENS","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"ENSUSDT-PERP"},{"contract_unit":"1 SUPER","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"SUPERUSDT-PERP"},{"contract_unit":"0.1 TRB","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.001","venue_symbol":"TRBUSDT-PERP"},{"contract_unit":"1 CKB","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"CKBUSDT-PERP"},{"contract_unit":"0.1 CELO","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"CELOUSDT-PERP"},{"contract_unit":"1 ARPA","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"ARPAUSDT-PERP"},{"contract_unit":"1 CFX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"CFXUSDT-PERP"},{"contract_unit":"1 BLUR","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"BLURUSDT-PERP"},{"contract_unit":"1 SAND","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"SANDUSDT-PERP"},{"contract_unit":"1 GRT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"GRTUSDT-PERP"},{"contract_unit":"0.1 OM","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"OMUSDT-PERP"},{"contract_unit":"1 ALT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"ALTUSDT-PERP"},{"contract_unit":"1 CHZ","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"CHZUSDT-PERP"},{"contract_unit":"1 GMT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"GMTUSDT-PERP"},{"contract_unit":"1 REEF","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.000001","venue_symbol":"REEFUSDT-PERP"},{"contract_unit":"1 LUNC","maximum_position_usdt":"1000000","minimum_lot":"1000000","price_tick":"0.00001","venue_symbol":"LUNCUSDT-PERP"},{"contract_unit":"1 KAS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"KASUSDT-PERP"},{"contract_unit":"1 HBAR","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"HBARUSDT-PERP"},{"contract_unit":"1 MINA","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"MINAUSDT-PERP"},{"contract_unit":"0.1 ALGO","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"ALGOUSDT-PERP"},{"contract_unit":"1 FIDA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"FIDAUSDT-PERP"},{"contract_unit":"1 XLM","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"XLMUSDT-PERP"},{"contract_unit":"1 AXS","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"AXSUSDT-PERP"},{"contract_unit":"1 YGG","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"YGGUSDT-PERP"},{"contract_unit":"0.1 THETA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"THETAUSDT-PERP"},{"contract_unit":"1 ALPHA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"ALPHAUSDT-PERP"},{"contract_unit":"1 WOO","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"WOOUSDT-PERP"},{"contract_unit":"1 STORJ","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"STORJUSDT-PERP"},{"contract_unit":"1 GHST","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"GHSTUSDT-PERP"},{"contract_unit":"1 MANA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"MANAUSDT-PERP"},{"contract_unit":"0.01 SSV","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"SSVUSDT-PERP"},{"contract_unit":"0.1 FLOW","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"FLOWUSDT-PERP"},{"contract_unit":"0.1 SNX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"SNXUSDT-PERP"},{"contract_unit":"1 1INCH","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"1INCHUSDT-PERP"},{"contract_unit":"1 ROSE","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"ROSEUSDT-PERP"},{"contract_unit":"1 SUN","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"SUNUSDT-PERP"},{"contract_unit":"1 USTC","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"USTCUSDT-PERP"},{"contract_unit":"0.1 BNX","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"BNXUSDT-PERP"},{"contract_unit":"1 VIDT","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"VIDTUSDT-PERP"},{"contract_unit":"0.01 NEO","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"NEOUSDT-PERP"},{"contract_unit":"1 BAKE","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"BAKEUSDT-PERP"},{"contract_unit":"1 BEAMX","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.000001","venue_symbol":"BEAMXUSDT-PERP"},{"contract_unit":"1 ID","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"IDUSDT-PERP"},{"contract_unit":"0.1 CYBER","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"CYBERUSDT-PERP"},{"contract_unit":"0.1 EGLD","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"EGLDUSDT-PERP"},{"contract_unit":"0.001 COMP","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.01","venue_symbol":"COMPUSDT-PERP"},{"contract_unit":"1 ETHW","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"ETHWUSDT-PERP"},{"contract_unit":"1 UMA","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"UMAUSDT-PERP"},{"contract_unit":"1 LINA","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.00001","venue_symbol":"LINAUSDT-PERP"},{"contract_unit":"0.1 API3","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"API3USDT-PERP"},{"contract_unit":"0.01 AUCTION","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"AUCTIONUSDT-PERP"},{"contract_unit":"0.1 LPT","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"LPTUSDT-PERP"},{"contract_unit":"1 BEL","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"BELUSDT-PERP"},{"contract_unit":"0.1 BSV","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.01","venue_symbol":"BSVUSDT-PERP"},{"contract_unit":"1 VANRY","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"VANRYUSDT-PERP"},{"contract_unit":"1 XEC","maximum_position_usdt":"1000000","minimum_lot":"10000000","price_tick":"0.00001","venue_symbol":"XECUSDT-PERP"},{"contract_unit":"1 VET","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"VETUSDT-PERP"},{"contract_unit":"0.1 DAR","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"DARUSDT-PERP"},{"contract_unit":"0.001 YFI","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"1","venue_symbol":"YFIUSDT-PERP"},{"contract_unit":"1 RSR","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"RSRUSDT-PERP"},{"contract_unit":"1 HOT","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.000001","venue_symbol":"HOTUSDT-PERP"},{"contract_unit":"1 ASTR","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.000001","venue_symbol":"ASTRUSDT-PERP"},{"contract_unit":"1 ZIL","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"ZILUSDT-PERP"},{"contract_unit":"0.1 ALICE","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"ALICEUSDT-PERP"},{"contract_unit":"1 COTI","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"COTIUSDT-PERP"},{"contract_unit":"1 CAKE","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"CAKEUSDT-PERP"},{"contract_unit":"1 ALPACA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"ALPACAUSDT-PERP"},{"contract_unit":"1 ONE","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"ONEUSDT-PERP"},{"contract_unit":"1 TRU","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"TRUUSDT-PERP"},{"contract_unit":"0.1 XTZ","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"XTZUSDT-PERP"},{"contract_unit":"1 MAV","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"MAVUSDT-PERP"},{"contract_unit":"0.1 MAGIC","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"MAGICUSDT-PERP"},{"contract_unit":"0.1 KAVA","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"KAVAUSDT-PERP"},{"contract_unit":"0.01 GMX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"GMXUSDT-PERP"},{"contract_unit":"1 C98","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"C98USDT-PERP"},{"contract_unit":"1 LRC","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"LRCUSDT-PERP"},{"contract_unit":"0.1 RPL","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"RPLUSDT-PERP"},{"contract_unit":"1 FLM","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"FLMUSDT-PERP"},{"contract_unit":"0.1 HIGH","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"HIGHUSDT-PERP"},{"contract_unit":"1 MTL","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"MTLUSDT-PERP"},{"contract_unit":"0.1 GAS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"GASUSDT-PERP"},{"contract_unit":"0.1 IOTA","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"IOTAUSDT-PERP"},{"contract_unit":"1 CHR","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"CHRUSDT-PERP"},{"contract_unit":"1 HIFI","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"HIFIUSDT-PERP"},{"contract_unit":"1 STMX","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.00001","venue_symbol":"STMXUSDT-PERP"},{"contract_unit":"1 EDU","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"EDUUSDT-PERP"},{"contract_unit":"1 IOTX","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"IOTXUSDT-PERP"},{"contract_unit":"0.1 ZRX","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"ZRXUSDT-PERP"},{"contract_unit":"1 ACH","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"ACHUSDT-PERP"},{"contract_unit":"0.1 KSM","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"KSMUSDT-PERP"},{"contract_unit":"1 GLM","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"GLMUSDT-PERP"},{"contract_unit":"0.1 ONT","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"ONTUSDT-PERP"},{"contract_unit":"1 MBOX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"MBOXUSDT-PERP"},{"contract_unit":"0.1 PERP","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"PERPUSDT-PERP"},{"contract_unit":"1 ENJ","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"ENJUSDT-PERP"},{"contract_unit":"1 LEVER","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.0000001","venue_symbol":"LEVERUSDT-PERP"},{"contract_unit":"0.1 GTC","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"GTCUSDT-PERP"},{"contract_unit":"1 TWT","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"TWTUSDT-PERP"},{"contract_unit":"1 STG","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"STGUSDT-PERP"},{"contract_unit":"0.1 LQTY","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"LQTYUSDT-PERP"},{"contract_unit":"1 DUSK","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"DUSKUSDT-PERP"},{"contract_unit":"0.1 QNT","maximum_position_usdt":"1000000","minimum_lot":"10","price_tick":"0.01","venue_symbol":"QNTUSDT-PERP"},{"contract_unit":"0.1 BAND","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"BANDUSDT-PERP"},{"contract_unit":"0.1 XVS","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"XVSUSDT-PERP"},{"contract_unit":"1 TLM","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"TLMUSDT-PERP"},{"contract_unit":"1 RVN","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"RVNUSDT-PERP"},{"contract_unit":"1 CTSI","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"CTSIUSDT-PERP"},{"contract_unit":"0.1 ILV","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"ILVUSDT-PERP"},{"contract_unit":"0.1 QTUM","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"QTUMUSDT-PERP"},{"contract_unit":"0.1 NMR","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.001","venue_symbol":"NMRUSDT-PERP"},{"contract_unit":"0.1 LIT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"LITUSDT-PERP"},{"contract_unit":"1 ANKR","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"ANKRUSDT-PERP"},{"contract_unit":"1 BLZ","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"BLZUSDT-PERP"},{"contract_unit":"1 CHESS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"CHESSUSDT-PERP"},{"contract_unit":"1 COS","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.000001","venue_symbol":"COSUSDT-PERP"},{"contract_unit":"0.1 FXS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"FXSUSDT-PERP"},{"contract_unit":"1 SPELL","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.0000001","venue_symbol":"SPELLUSDT-PERP"},{"contract_unit":"1 SKL","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"SKLUSDT-PERP"},{"contract_unit":"0.1 BAT","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"BATUSDT-PERP"},{"contract_unit":"1 DENT","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.000001","venue_symbol":"DENTUSDT-PERP"},{"contract_unit":"1 POWR","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"POWRUSDT-PERP"},{"contract_unit":"1 CELR","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"CELRUSDT-PERP"},{"contract_unit":"1 SYS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"SYSUSDT-PERP"},{"contract_unit":"1 SFP","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"SFPUSDT-PERP"},{"contract_unit":"1 FLUX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"FLUXUSDT-PERP"},{"contract_unit":"1 IOST","maximum_position_usdt":"1000000","minimum_lot":"100000","price_tick":"0.000001","venue_symbol":"IOSTUSDT-PERP"},{"contract_unit":"1 ONG","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"ONGUSDT-PERP"},{"contract_unit":"0.1 BAL","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"BALUSDT-PERP"},{"contract_unit":"0.01 MOVR","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"MOVRUSDT-PERP"},{"contract_unit":"1 ATA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"ATAUSDT-PERP"},{"contract_unit":"1 VOXEL","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"VOXELUSDT-PERP"},{"contract_unit":"1 QUICK","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"QUICKUSDT-PERP"},{"contract_unit":"1 KNC","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"KNCUSDT-PERP"},{"contract_unit":"1 ICX","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"ICXUSDT-PERP"},{"contract_unit":"1 BADGER","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"BADGERUSDT-PERP"},{"contract_unit":"1 NKN","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"NKNUSDT-PERP"},{"contract_unit":"0.1 RLC","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"RLCUSDT-PERP"},{"contract_unit":"1 T","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"TUSDT-PERP"},{"contract_unit":"1 HFT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"HFTUSDT-PERP"},{"contract_unit":"1 JOE","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"JOEUSDT-PERP"},{"contract_unit":"1 AERGO","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"AERGOUSDT-PERP"},{"contract_unit":"1 BSW","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"BSWUSDT-PERP"},{"contract_unit":"1 LOKA","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"LOKAUSDT-PERP"},{"contract_unit":"1 RIF","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"RIFUSDT-PERP"},{"contract_unit":"0.1 OMG","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"OMGUSDT-PERP"},{"contract_unit":"1 NULS","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.0001","venue_symbol":"NULSUSDT-PERP"},{"contract_unit":"1 WAXP","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.00001","venue_symbol":"WAXPUSDT-PERP"},{"contract_unit":"1 STEEM","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"STEEMUSDT-PERP"},{"contract_unit":"1 LSK","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"LSKUSDT-PERP"},{"contract_unit":"1 BNT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"BNTUSDT-PERP"},{"contract_unit":"0.1 BOND","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.001","venue_symbol":"BONDUSDT-PERP"},{"contract_unit":"0.1 COMBO","maximum_position_usdt":"1000000","minimum_lot":"10000","price_tick":"0.0001","venue_symbol":"COMBOUSDT-PERP"},{"contract_unit":"1 OXT","maximum_position_usdt":"1000000","minimum_lot":"1000","price_tick":"0.00001","venue_symbol":"OXTUSDT-PERP"},{"contract_unit":"1 KDA","maximum_position_usdt":"1000000","minimum_lot":"100","price_tick":"0.0001","venue_symbol":"KDAUSDT-PERP"}]"#;
pub(crate) const PLUANG_LISTINGS_JSON: &str = r#"{"snapshot_date":"2026-09-25","spot_source_url":"https://pluang.com/en/explore/crypto/spot","futures_source_url":"https://pluang.com/en/explore/crypto/futures","spot_symbols":"BTC ETH USDT BNB XRP USDC SOL TRX STETH HYPE DOGE LINK WBETH WBTC USDS ADA RAIN XLM BCH NEAR UNI LTC CC AVAX SUI HBAR GRAM TAO SHIB CRO PYUSD XAUT ONDO USDY ENA AAVE MNT DOT ASTER PAXG PUMP PEPE WLFI ICP SKY WLD HTX MORPHO ETC VVV ARB BNSOL POL KAS QNT JUP ALGO KCS JST RENDER ATOM CAKE FIL DASH INJ VET STABLE AERO APT ETHFI XDC METH FLR PENGU TRUMP NEXO STX ZRO RAY PYTH CRV FET VIRTUAL PIEVERSE USDTB SEI PENDLE TIA FF SPX NIGHT BTTC LDO UB XTZ SUN FDUSD BONK DCR KITE XPL IMX GNO BR LUNC OP ENS AR STRK USELESS GRT CFX FLOKI JTO SYRUP TWT NFT WIF COMP MIOTA THETA JASMY ZAMA EIGEN KAIA RUNE KMNO CVX ZBCN AXS XCN 2Z MINA FARTCOIN MET NEO XEC MANA NPC CHZ MX A DEXE AIOZ TEL SENT GOMINING APE SFP 1INCH EGLD BAT ZK SAND AWE H GLM DEEP ATH Q FLUID GENIUS PLUME S FORM DYDX SUPER QTUM GRASS RSR SOSO 1000CHEEMS BULLA BEAMX ZRX SKR GALA PROM WEMIX ORCA ORDI GAS CHIP RIF HUMA SNX WAL YFI RED CFG KSM SAFE GMX KAITO COW LSK UAI VELO DGB LPT BANANAS31 TFUEL ZETA DATA1 VTHO NXPC HOT KAVA W BERA BIO ZIL ZIG 1MBABYDOGE NMR SUSHI ARC TURBO BOME CKB GPS SPK BAN G AURORA LINEA AXL ROSE CTC ASTR IO ACH T BABY ID BLUR NIL ONT ALLO SC TRB SOMI CELO BRETT ENJ JELLYJELLY ALT POPCAT TOSHI XVS PNUT HOLO 0G FLOW SYN BARD XVG SSV XNO COTI ANKR MOG MOODENG AZTEC NOT RONIN PEOPLE MASK DBR PROVE AMP ONG ALCH ME OPEN REQ RPL MEW MOCA DUSK KUB PHA MUBARAK IRYS APEX EDU GWEI MOVE AVNT MEME GIGGLE MERL BAND RVN WIN ZORA POWR LISTA AT LUNA EURI NEIRO UMA LAYER ONE C BLAST SAHARA CORE REZ MMT BNT EUL FLOCK SQD CATI IOTX PRO MANTA B2 HIVE IOST ZEREBRO CPOOL USTC HYPER BEAT MTL SIGN PUNDIX XAN CVC KNC MANTRA OSMO ARKM OPG RLC FLUX FOLKS CARV ZBT FRAX SKL WAXP VELODROME METIS CTSI BROCCOLI714 ILV HOME USUAL CYBER GMT CETUS BANK EDEN AUCTION DEGEN IQ ICNT STRAX VR XNY YGG SLP 1000SATS DOGS BIGTIME WOO AEVO CELR API3 SOLV LQTY ZKC BICO AIXBT CTK BREV MPLX TUT TA QKC FIDA SONIC NYM ENSO INIT CHR ACE CGPT AUDIO TNSR AGLD STBL QI SAPIEN TAIKO BIRB PIXEL AVA XAI WCT GOAT SIREN ANIME DODO MIRA F SPELL DIA 1000CAT STG JOE ARPA NEWT EPIC MAGIC YB MBL ORDER BANANA C98 HEMI PORTAL LUMIA PUFFER KERNEL PARTI ZKP BMT ALICE ZENT RAD ELSA DOOD LA OG MAV CORN AGI OGN DOLO SAGA SXT CHILLGUY DYM BB RECALL FHE SHELL AI HEI WET DRIFT HMSTR THE MOVR GNS RARE HAEDAL GUN TLM FORT BEL ACT ERA BOBA ATM AVAAI STO COOKIE NAVX MBX SD ADX ACS AIO RESOLV MAVIA SANTOS TRIA PRIME ESE PSG GLMR BAR LIGHT ASR FIGHT CLO TOWNS SUPRA SOPH TREE ALPINE GTC TLOS TURTLE QUICK PORTO NOM IKA ZKJ LMWR JUV CITY LAZIO ROAM NS SCR ACM AVL A8 MAJOR MITO OBT CXT MCRT XMN MEE CLOUD RATS","futures_symbols":"BTCUSDT-PERP ETHUSDT-PERP BNBUSDT-PERP XRPUSDT-PERP SOLUSDT-PERP TRXUSDT-PERP HYPEUSDT-PERP DOGEUSDT-PERP LINKUSDT-PERP ADAUSDT-PERP XLMUSDT-PERP BCHUSDT-PERP NEARUSDT-PERP UNIUSDT-PERP LTCUSDT-PERP AVAXUSDT-PERP SUIUSDT-PERP HBARUSDT-PERP TAOUSDT-PERP SHIBUSDT-PERP XAUTUSDT-PERP ONDOUSDT-PERP ENAUSDT-PERP AAVEUSDT-PERP DOTUSDT-PERP ASTERUSDT-PERP PUMPUSDT-PERP PEPEUSDT-PERP WLFIUSDT-PERP ICPUSDT-PERP WLDUSDT-PERP ARBUSDT-PERP POLUSDT-PERP KASUSDT-PERP QNTUSDT-PERP RENDERUSDT-PERP ATOMUSDT-PERP FILUSDT-PERP INJUSDT-PERP APTUSDT-PERP PENGUUSDT-PERP FETUSDT-PERP PENDLEUSDT-PERP XPLUSDT-PERP OPUSDT-PERP FLOKIUSDT-PERP BATUSDT-PERP ZRXUSDT-PERP SNXUSDT-PERP"}"#;

pub(crate) const REGISTRY_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS instrument_registry_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS exchanges (
    exchange_id TEXT PRIMARY KEY,
    exchange_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    exchange_kind TEXT NOT NULL DEFAULT 'unknown',
    status TEXT NOT NULL DEFAULT 'enabled'
);
CREATE TABLE IF NOT EXISTS exchange_products (
    exchange_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    asset_category_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'enabled',
    PRIMARY KEY(exchange_id, product_id),
    FOREIGN KEY(exchange_id) REFERENCES exchanges(exchange_id)
);
CREATE TABLE IF NOT EXISTS data_providers (
    provider_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    provider_type TEXT NOT NULL DEFAULT 'unclassified',
    status TEXT NOT NULL DEFAULT 'enabled'
);
CREATE TABLE IF NOT EXISTS plan_profiles (
    profile_id TEXT PRIMARY KEY,
    exchange_id TEXT NOT NULL DEFAULT '',
    product_id TEXT NOT NULL DEFAULT '',
    strategy_id TEXT NOT NULL DEFAULT '',
    timeframe TEXT NOT NULL DEFAULT '',
    execution_scope TEXT NOT NULL DEFAULT 'research_only',
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE IF NOT EXISTS listing_sync_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    exchange_id TEXT NOT NULL,
    product_id TEXT NOT NULL DEFAULT '',
    source_url TEXT NOT NULL,
    attempted_at TEXT NOT NULL,
    completed_at TEXT,
    snapshot_at TEXT,
    status TEXT NOT NULL,
    records_received INTEGER NOT NULL DEFAULT 0,
    error TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_listing_sync_product_time
    ON listing_sync_runs(exchange_id, product_id, attempted_at DESC);
CREATE TABLE IF NOT EXISTS instruments (
    instrument_id TEXT PRIMARY KEY,
    exchange_id TEXT NOT NULL,
    product_id TEXT NOT NULL DEFAULT '',
    asset_category_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    venue_symbol TEXT NOT NULL,
    underlying_symbol TEXT NOT NULL,
    storage_symbol TEXT NOT NULL,
    quote_currency TEXT NOT NULL,
    listing_status TEXT NOT NULL,
    venue_verified INTEGER NOT NULL DEFAULT 0,
    execution_supported INTEGER NOT NULL DEFAULT 0,
    registration_source TEXT NOT NULL,
    source_url TEXT,
    source_snapshot_at TEXT,
    price_source_policy TEXT NOT NULL,
    contract_metadata_json TEXT NOT NULL DEFAULT '{}',
    registered_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(exchange_id, product_id, symbol)
);
CREATE INDEX IF NOT EXISTS idx_instruments_exchange_active
    ON instruments(exchange_id, is_active, asset_category_id, symbol);
CREATE INDEX IF NOT EXISTS idx_instruments_active
    ON instruments(is_active);
CREATE INDEX IF NOT EXISTS idx_instruments_storage_symbol
    ON instruments(storage_symbol, is_active);
CREATE INDEX IF NOT EXISTS idx_instruments_exchange_storage_upper
    ON instruments(exchange_id, is_active, upper(storage_symbol), venue_verified);
CREATE INDEX IF NOT EXISTS idx_instruments_exchange_symbol_upper
    ON instruments(exchange_id, is_active, upper(symbol), venue_verified);
CREATE INDEX IF NOT EXISTS idx_instruments_plan_storage_eligibility
    ON instruments(exchange_id, product_id, is_active, venue_verified, storage_symbol, listing_status);
CREATE INDEX IF NOT EXISTS idx_instruments_plan_symbol_eligibility
    ON instruments(exchange_id, product_id, is_active, venue_verified, symbol, listing_status);
CREATE TABLE IF NOT EXISTS instrument_exclusions (
    exclusion_id INTEGER PRIMARY KEY AUTOINCREMENT,
    exchange_id TEXT NOT NULL,
    asset_category_id TEXT NOT NULL,
    symbol TEXT NOT NULL COLLATE NOCASE,
    reason TEXT NOT NULL,
    source TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK(is_active IN (0, 1)),
    UNIQUE(exchange_id, asset_category_id, symbol)
);
CREATE INDEX IF NOT EXISTS idx_instrument_exclusions_active
    ON instrument_exclusions(exchange_id, asset_category_id, is_active, symbol);
CREATE TABLE IF NOT EXISTS plan_evidence (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trade_plan_id INTEGER NOT NULL,
    evidence_source TEXT NOT NULL DEFAULT '',
    execution_scope TEXT NOT NULL DEFAULT 'unknown',
    validation_status TEXT NOT NULL DEFAULT 'pending',
    venue_native INTEGER NOT NULL DEFAULT 0 CHECK(venue_native IN (0,1)),
    sample_count INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(trade_plan_id),
    FOREIGN KEY(trade_plan_id) REFERENCES trade_plans(id) ON DELETE CASCADE
);
"#;

pub(crate) fn initialize_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    let schema_name = registry_schema(conn)?;
    let table_qualifier = format!("CREATE TABLE IF NOT EXISTS {schema_name}.");
    let index_qualifier = format!("CREATE INDEX IF NOT EXISTS {schema_name}.");
    let schema = REGISTRY_SCHEMA
        .replace("CREATE TABLE IF NOT EXISTS ", &table_qualifier)
        .replace("CREATE INDEX IF NOT EXISTS ", &index_qualifier);
    conn.execute_batch(&schema)
}

pub(crate) fn initialize(conn: &Connection) -> Result<(), rusqlite::Error> {
    initialize_schema(conn)?;
    conn.execute(
        "INSERT OR IGNORE INTO reference_catalogs(catalog_id, payload_json, source_label)
         VALUES('pintu_futures_contracts', ?1, 'embedded_release_seed')",
        [PINTU_CONTRACTS_JSON],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO reference_catalogs(catalog_id, payload_json, source_label)
         VALUES('pluang_crypto_listings', ?1, 'embedded_release_seed')",
        [PLUANG_LISTINGS_JSON],
    )?;
    migrate_contract_metadata(conn)?;
    conn.execute(
        r#"INSERT OR IGNORE INTO instrument_exclusions (
               exchange_id, asset_category_id, symbol, reason, source,
               created_at, updated_at, is_active
           ) VALUES ('stockbit', 'idx_stock', 'ACRO',
                     'User requested exclusion after losses', 'user_request',
                     unixepoch(), unixepoch(), 1)"#,
        [],
    )?;

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        seed_pintu(conn)?;
        seed_pluang(conn)?;
        conn.execute(
            "DELETE FROM instrument_registry_meta WHERE key='seed_version'",
            [],
        )?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error);
        }
    }

    // Earlier releases inferred Stockbit app registrations from observed
    // `.JK` price history. Price data cannot establish an app listing.
    // Retire only those derived registrations; bars and series remain canonical
    // market history in the same database.
    let retired = conn
        .query_row(
            "SELECT value FROM instrument_registry_meta
          WHERE key='history_registration_retired_v1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if retired.is_none() {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let retirement = (|| {
            let already_done = conn
                .query_row(
                    "SELECT value FROM instrument_registry_meta
                  WHERE key='history_registration_retired_v1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if already_done.is_none() {
                let count = conn.execute(
                    "DELETE FROM instruments
                      WHERE exchange_id='stockbit'
                        AND registration_source='local_market_history'",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO instrument_registry_meta(key,value)
                     VALUES('history_registration_retired_v1',?1)",
                    [count.to_string()],
                )?;
            }
            Ok::<(), rusqlite::Error>(())
        })();
        match retirement {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
    }

    sync_normalized_dimensions(conn)?;
    Ok(())
}

pub(crate) fn product_display_name(product_id: &str) -> String {
    match product_id {
        "pintu_retail_spot" => "Retail Spot".into(),
        "pintu_pro_spot" => "Pro Spot".into(),
        "pintu_pro_futures" => "Pro Futures".into(),
        "pluang_spot" => "Spot".into(),
        "pluang_perpetual" => "Perpetual".into(),
        "" => "Default product".into(),
        value => value
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub(crate) fn sync_normalized_dimensions(conn: &Connection) -> Result<(), rusqlite::Error> {
    for exchange in crate::master_data::master_data_records("exchanges") {
        let id = crate::master_data::catalog_string(exchange, "id").unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let code = crate::master_data::catalog_string(exchange, "code").unwrap_or(id);
        let label = crate::master_data::catalog_string(exchange, "label")
            .or_else(|| crate::master_data::catalog_string(exchange, "name"))
            .unwrap_or(id);
        let kind = crate::master_data::catalog_string(exchange, "kind").unwrap_or("unknown");
        let status = crate::master_data::catalog_string(exchange, "status").unwrap_or("enabled");
        conn.execute(
            "INSERT INTO exchanges(exchange_id,exchange_code,display_name,exchange_kind,status)
             VALUES(?1,?2,?3,?4,?5) ON CONFLICT(exchange_id) DO UPDATE SET
             exchange_code=excluded.exchange_code,display_name=excluded.display_name,
             exchange_kind=excluded.exchange_kind,status=excluded.status",
            params![id, code, label, kind, status],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO exchanges(exchange_id,exchange_code,display_name)
         SELECT DISTINCT exchange_id,upper(exchange_id),exchange_id FROM instruments",
        [],
    )?;

    let products = {
        let mut stmt = conn.prepare(
            "SELECT exchange_id,product_id,asset_category_id FROM instruments
             GROUP BY exchange_id,product_id,asset_category_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (exchange_id, product_id, category_id) in products {
        conn.execute(
            "INSERT INTO exchange_products(exchange_id,product_id,display_name,asset_category_id)
             VALUES(?1,?2,?3,?4) ON CONFLICT(exchange_id,product_id) DO UPDATE SET
             display_name=excluded.display_name,asset_category_id=excluded.asset_category_id",
            params![
                exchange_id,
                product_id,
                product_display_name(&product_id),
                category_id
            ],
        )?;
    }
    for profile in crate::master_data::master_data_records("plan_profiles") {
        let profile_id = crate::master_data::catalog_string(profile, "id").unwrap_or("");
        if profile_id.is_empty() {
            continue;
        }
        let exchange_id = crate::master_data::catalog_string(profile, "exchange_id").unwrap_or("");
        let product_id = crate::master_data::catalog_string(profile, "product_id").unwrap_or("");
        if !exchange_id.is_empty() {
            conn.execute(
                "INSERT OR IGNORE INTO exchanges(exchange_id,exchange_code,display_name)
                 VALUES(?1,upper(?1),?1)",
                [exchange_id],
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO exchange_products(exchange_id,product_id,display_name)
                 VALUES(?1,?2,?3)",
                params![exchange_id, product_id, product_display_name(product_id)],
            )?;
        }
        conn.execute(
            "INSERT INTO plan_profiles(profile_id,exchange_id,product_id,strategy_id,timeframe,
                execution_scope,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,unixepoch()) ON CONFLICT(profile_id) DO UPDATE SET
                exchange_id=excluded.exchange_id,product_id=excluded.product_id,
                strategy_id=excluded.strategy_id,timeframe=excluded.timeframe,
                execution_scope=excluded.execution_scope,
                updated_at=excluded.updated_at",
            params![
                profile_id,
                exchange_id,
                product_id,
                crate::master_data::catalog_string(profile, "engine_strategy_id")
                    .or_else(|| crate::master_data::catalog_string(profile, "signal_strategy_id"))
                    .unwrap_or(profile_id),
                crate::master_data::catalog_string(profile, "timeframe").unwrap_or(""),
                crate::master_data::catalog_string(profile, "execution_scope")
                    .unwrap_or("research_only")
            ],
        )?;
    }

    for provider in [
        "binance",
        "coinbase",
        "bybit",
        "okx",
        "yfinance",
        "tokocrypto",
        "scheduler",
        "unknown",
    ] {
        let provider_type = match provider {
            "binance" | "coinbase" | "bybit" | "okx" => "reference_exchange",
            "yfinance" => "research_proxy",
            "tokocrypto" => "venue_adapter",
            "scheduler" => "scheduler",
            _ => "unclassified",
        };
        conn.execute(
            "INSERT OR IGNORE INTO data_providers(provider_id,display_name,provider_type)
             VALUES(?1,?1,?2)",
            params![provider, provider_type],
        )?;
    }
    let has_market_history: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema
                       WHERE type='table' AND name='series')",
        [],
        |row| row.get(0),
    )?;
    if has_market_history {
        conn.execute_batch(
            "INSERT OR IGNORE INTO data_providers(provider_id,display_name)
                 SELECT data_provider,data_provider FROM main.series WHERE data_provider<>'';
             INSERT OR IGNORE INTO data_providers(provider_id,display_name)
                 SELECT data_provider,data_provider FROM main.ingestion_runs WHERE data_provider<>'';",
        )?;
    }
    Ok(())
}

/// Maintainer-run schema/data migration. Never invoked by normal startup.
/// All registry and plan identity changes commit together; historical bars and
/// their original source labels remain untouched as provenance.
pub(crate) fn migrate_app_products(conn: &Connection) -> Result<usize, String> {
    let schema_name = registry_schema(conn).map_err(|error| error.to_string())?;
    let has_product = {
        let mut stmt = conn
            .prepare(&format!("PRAGMA {schema_name}.table_info('instruments')"))
            .map_err(|e| e.to_string())?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| e.to_string())?;
        let found = names
            .filter_map(Result::ok)
            .any(|name| name == "product_id");
        found
    };
    if has_product {
        return Ok(0);
    }
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<usize, String> {
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM instruments", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let migration_sql = r#"
            CREATE TABLE app.instruments_product_migration (
                instrument_id TEXT PRIMARY KEY, exchange_id TEXT NOT NULL,
                product_id TEXT NOT NULL DEFAULT '', asset_category_id TEXT NOT NULL,
                symbol TEXT NOT NULL, venue_symbol TEXT NOT NULL,
                underlying_symbol TEXT NOT NULL, storage_symbol TEXT NOT NULL,
                quote_currency TEXT NOT NULL, listing_status TEXT NOT NULL,
                venue_verified INTEGER NOT NULL DEFAULT 0,
                execution_supported INTEGER NOT NULL DEFAULT 0,
                registration_source TEXT NOT NULL, source_url TEXT,
                source_snapshot_at TEXT, price_source_policy TEXT NOT NULL,
                contract_metadata_json TEXT NOT NULL DEFAULT '{}',
                registered_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 1,
                UNIQUE(exchange_id, product_id, symbol)
            );
            INSERT INTO app.instruments_product_migration
              SELECT CASE WHEN exchange_id='pintu_futures'
                          THEN 'pintu.pintu_pro_futures.' || lower(symbol)
                          WHEN exchange_id='pluang'
                          THEN 'pluang.' || CASE WHEN asset_category_id='crypto_spot'
                                   THEN 'pluang_spot' ELSE 'pluang_perpetual' END || '.' || lower(symbol)
                          ELSE instrument_id END,
                     CASE WHEN exchange_id='pintu_futures' THEN 'pintu' ELSE exchange_id END,
                     CASE WHEN exchange_id='pintu_futures' THEN 'pintu_pro_futures'
                          WHEN exchange_id='pluang' AND asset_category_id='crypto_spot' THEN 'pluang_spot'
                          WHEN exchange_id='pluang' THEN 'pluang_perpetual' ELSE '' END,
                     asset_category_id,symbol,venue_symbol,underlying_symbol,storage_symbol,
                     quote_currency,listing_status,venue_verified,execution_supported,
                     registration_source,source_url,source_snapshot_at,price_source_policy,
                     contract_metadata_json,registered_at,updated_at,is_active
                FROM instruments;
            DROP TABLE app.instruments;
            ALTER TABLE app.instruments_product_migration RENAME TO instruments;
            CREATE INDEX IF NOT EXISTS app.idx_instruments_exchange_active
                ON instruments(exchange_id,is_active,asset_category_id,symbol);
            CREATE INDEX IF NOT EXISTS app.idx_instruments_active
                ON instruments(is_active);
            CREATE INDEX IF NOT EXISTS app.idx_instruments_storage_symbol
                ON instruments(storage_symbol,is_active);
            CREATE INDEX IF NOT EXISTS app.idx_instruments_exchange_storage_upper
                ON instruments(exchange_id,is_active,upper(storage_symbol),venue_verified);
            CREATE INDEX IF NOT EXISTS app.idx_instruments_exchange_symbol_upper
                ON instruments(exchange_id,is_active,upper(symbol),venue_verified);
            CREATE INDEX IF NOT EXISTS app.idx_instruments_plan_storage_eligibility
                ON instruments(exchange_id,product_id,is_active,venue_verified,storage_symbol,listing_status);
            CREATE INDEX IF NOT EXISTS app.idx_instruments_plan_symbol_eligibility
                ON instruments(exchange_id,product_id,is_active,venue_verified,symbol,listing_status);
            UPDATE series SET exchange_id='pintu' WHERE exchange_id='pintu_futures';
            UPDATE ingestion_runs SET exchange_id='pintu' WHERE exchange_id='pintu_futures';
        "#
        .replace("app.", &format!("{schema_name}."));
        conn.execute_batch(&migration_sql)
            .map_err(|e| e.to_string())?;
        let copied: usize = conn
            .query_row("SELECT COUNT(*) FROM instruments", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if copied != count {
            return Err("registry migration row count mismatch".into());
        }
        let plan_rows = {
            let mut statement = conn
                .prepare("SELECT id,plan_json FROM trade_plans")
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| error.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        };
        for (id, encoded) in plan_rows {
            let mut plan: Value =
                crate::wire::decode(&encoded).map_err(|error| error.to_string())?;
            let is_legacy_pintu = plan
                .get("Exchange")
                .and_then(Value::as_str)
                .is_some_and(|exchange| exchange.eq_ignore_ascii_case("pintu_futures"));
            if !is_legacy_pintu {
                continue;
            }
            if let Some(object) = plan.as_object_mut() {
                object.insert("Exchange".into(), json!("PINTU"));
                object.insert("Product_ID".into(), json!("pintu_pro_futures"));
                let symbol = object
                    .get("Data_Symbol")
                    .or_else(|| object.get("Symbol"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                object.insert(
                    "Plan_ID".into(),
                    json!(format!("pintu::pintu_pro_futures::{symbol}")),
                );
            }
            let updated = crate::wire::encode(&plan).map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE trade_plans SET plan_json=?1 WHERE id=?2",
                params![updated, id],
            )
            .map_err(|error| error.to_string())?;
        }
        conn.execute("INSERT OR REPLACE INTO instrument_registry_meta(key,value) VALUES('app_products_migration_v1',?1)",
            [count.to_string()]).map_err(|e| e.to_string())?;
        Ok(count)
    })();
    match result {
        Ok(count) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(count)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

pub(crate) fn instrument_column_exists(
    conn: &Connection,
    name: &str,
) -> Result<bool, rusqlite::Error> {
    let schema_name = registry_schema(conn)?;
    let mut statement = conn.prepare(&format!("PRAGMA {schema_name}.table_info('instruments')"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == name {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn migrate_contract_metadata(conn: &Connection) -> Result<(), rusqlite::Error> {
    let has_json = instrument_column_exists(conn, "contract_metadata_json")?;
    if has_json {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "ALTER TABLE {}.instruments ADD COLUMN contract_metadata_json TEXT NOT NULL DEFAULT '{{}}'",
        registry_schema(conn)?
    ))
}

fn registry_schema(conn: &Connection) -> Result<&'static str, rusqlite::Error> {
    let mut statement = conn.prepare("PRAGMA database_list")?;
    let names = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == "app" {
            return Ok("app");
        }
    }
    Ok("main")
}

pub(crate) fn is_excluded(
    conn: &Connection,
    exchange_id: &str,
    asset_category_id: &str,
    symbol: &str,
) -> Result<bool, rusqlite::Error> {
    let mut normalized_symbol = symbol.trim().to_ascii_uppercase();
    if let Some(base) = normalized_symbol.strip_suffix(".JK") {
        normalized_symbol = base.to_string();
    }
    conn.query_row(
        r#"SELECT EXISTS(
               SELECT 1 FROM instrument_exclusions
                WHERE exchange_id = ?1
                  AND asset_category_id = ?2
                  AND symbol = ?3
                  AND is_active = 1
           )"#,
        params![
            exchange_id.trim().to_ascii_lowercase(),
            asset_category_id.trim().to_ascii_lowercase(),
            normalized_symbol
        ],
        |row| row.get(0),
    )
}
