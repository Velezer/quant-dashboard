use super::*;
use crate::tokocrypto::VenueSymbol;

fn fixture_database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();
    crate::database::initialize_app_storage_schema(&connection).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE series(
            symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,product_id TEXT DEFAULT '',
            instrument_id TEXT,data_provider TEXT,timezone TEXT,price_basis TEXT,
            instrument_kind TEXT,last_success TEXT,last_attempt TEXT,last_error TEXT);
         CREATE TABLE bars(symbol TEXT,interval TEXT,source TEXT,ts INTEGER,
            open REAL,high REAL,low REAL,close REAL,volume REAL);
         CREATE TABLE ingestion_runs(symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,
            product_id TEXT DEFAULT '',data_provider TEXT,attempted_at TEXT,status TEXT,
            rows_received INTEGER,error TEXT);",
        )
        .unwrap();
    crate::master_data::initialize(&connection)
        .expect("test database should initialize canonical reference data");
    connection
}

fn stockbit_listings() -> Vec<super::super::stockbit_listing::Listing> {
    let mut listings = vec![super::super::stockbit_listing::Listing {
        symbol: "BBCA".into(),
    }];
    listings.extend(
        (0..499).map(|index| super::super::stockbit_listing::Listing {
            symbol: format!("S{index:04}"),
        }),
    );
    listings
}

#[test]
fn stockbit_partial_refresh_preserves_registry_and_complete_refresh_is_idempotent() {
    let connection = fixture_database();
    connection
        .execute(
            "INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,listing_status,
                venue_verified,execution_supported,registration_source,price_source_policy,
                contract_metadata_json,registered_at,updated_at,is_active)
             VALUES('stockbit.idx_stock.bbca','stockbit','','idx_stock','BBCA','BBCA','BBCA',
                'BBCA.JK','IDR','manual_unverified',0,0,'dashboard_manual_registration',
                'yfinance_research_proxy','{}',1,1,1)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,instrument_id,
                data_provider,timezone,price_basis,instrument_kind)
             VALUES('BBCA.JK','1d','stockbit_yfinance_fallback','stockbit','',
                'stockbit.idx_stock.bbca','yfinance','Asia/Jakarta','research_proxy','idx_stock')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume)
             VALUES('BBCA.JK','1d','stockbit_yfinance_fallback',1,10,12,9,11,100)",
            [],
        )
        .unwrap();

    let partial = reconcile_stockbit(
        &connection,
        &stockbit_listings()[..super::super::stockbit_listing::MIN_LISTING_SYMBOLS - 1],
    );
    assert!(partial.error.is_some());
    let old: (String, i64) = connection
        .query_row(
            "SELECT listing_status,venue_verified FROM instruments WHERE instrument_id='stockbit.idx_stock.bbca'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(old, ("manual_unverified".into(), 0));

    let complete = stockbit_listings();
    let first = reconcile_stockbit(&connection, &complete);
    assert!(first.error.is_none());
    assert_eq!(
        first.inserted,
        super::super::stockbit_listing::MIN_LISTING_SYMBOLS
    );
    let second = reconcile_stockbit(&connection, &complete);
    assert!(second.error.is_none());
    assert_eq!(second.inserted, 0);
    let bbca: (String, String, i64) = connection
        .query_row(
            "SELECT product_id,listing_status,venue_verified FROM instruments
              WHERE instrument_id='stockbit.stockbit_regular.bbca'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        bbca,
        (
            "stockbit_regular".into(),
            "public_listed_snapshot".into(),
            1
        )
    );
    let provenance: (String, String, String, String, String, i64) = connection
        .query_row(
            "SELECT i.product_id,i.instrument_id,i.registration_source,i.source_url,
                    i.price_source_policy,(SELECT COUNT(*) FROM bars WHERE symbol='BBCA.JK')
               FROM instruments i JOIN series s ON s.instrument_id=i.instrument_id
              WHERE s.symbol='BBCA.JK' AND s.interval='1d'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(provenance.0, "stockbit_regular");
    assert_eq!(provenance.1, "stockbit.stockbit_regular.bbca");
    assert_eq!(provenance.2, "idx_listed_company_directory");
    assert_eq!(
        provenance.3,
        crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL
    );
    assert_eq!(provenance.4, "yfinance_research_proxy");
    assert_eq!(provenance.5, 1);
}

#[test]
fn stockbit_duplicate_refresh_keeps_previous_verified_registration() {
    let connection = fixture_database();
    connection
        .execute(
            "INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,listing_status,
                venue_verified,execution_supported,registration_source,source_url,
                price_source_policy,contract_metadata_json,registered_at,updated_at,is_active)
             VALUES('stockbit.stockbit_regular.bbca','stockbit','stockbit_regular','idx_stock',
                'BBCA','BBCA','BBCA','BBCA.JK','IDR','public_listed_snapshot',1,0,
                'idx_listed_company_directory',?1,'yfinance_research_proxy','{}',1,1,1)",
            [crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL],
        )
        .unwrap();
    let mut listings = stockbit_listings();
    listings[1].symbol = listings[0].symbol.clone();

    let result = reconcile_stockbit(&connection, &listings);
    assert!(result.error.as_deref().unwrap().contains("repeats ticker"));
    let state: (String, bool) = connection
        .query_row(
            "SELECT listing_status,is_active FROM instruments
             WHERE instrument_id='stockbit.stockbit_regular.bbca'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, ("public_listed_snapshot".into(), true));
}

#[test]
fn tokocrypto_spot_and_tokenized_products_keep_distinct_app_identity() {
    let connection = fixture_database();
    let symbols = vec![
        VenueSymbol {
            symbol: "SAME_IDR".into(),
            symbol_type: 1,
            base_asset: "SAME".into(),
            quote_asset: "IDR".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "TSLA_USDT".into(),
            symbol_type: 3,
            base_asset: "TSLA".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "ALCH_IDR".into(),
            symbol_type: 3,
            base_asset: "ALCH".into(),
            quote_asset: "IDR".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "FARTCOIN_USDT".into(),
            symbol_type: 3,
            base_asset: "FARTCOIN".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
    ];
    let first = reconcile_tokocrypto(&connection, &symbols);
    assert!(first.error.is_none(), "{:?}", first.error);
    assert_eq!(first.inserted, 4);
    let second = reconcile_tokocrypto(&connection, &symbols);
    assert!(second.error.is_none());
    assert_eq!(second.inserted, 0);
    let mut statement = connection
        .prepare(
            "SELECT product_id,asset_category_id,storage_symbol,listing_status,venue_verified
               FROM instruments WHERE exchange_id='tokocrypto' ORDER BY product_id,symbol",
        )
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "tokocrypto_spot".into(),
                "crypto_spot".into(),
                "TOKOCRYPTO:SPOT:ALCH_IDR".into(),
                "venue_active".into(),
                1,
            ),
            (
                "tokocrypto_spot".into(),
                "crypto_spot".into(),
                "TOKOCRYPTO:SPOT:FARTCOIN_USDT".into(),
                "venue_active".into(),
                1,
            ),
            (
                "tokocrypto_spot".into(),
                "crypto_spot".into(),
                "TOKOCRYPTO:SPOT:SAME_IDR".into(),
                "venue_active".into(),
                1,
            ),
            (
                "tokocrypto_tokenized_stock".into(),
                "tokenized_stock".into(),
                "TOKOCRYPTO:TOKENIZED:TSLA_USDT".into(),
                "venue_active".into(),
                1,
            ),
        ]
    );

    let failed = reconcile_tokocrypto(&connection, &[]);
    assert!(failed.error.is_some());
    let still_confirmed: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM instruments WHERE exchange_id='tokocrypto'
                AND listing_status='venue_active' AND venue_verified=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(still_confirmed, 4);
}

#[test]
fn tokocrypto_next_rows_and_duplicate_stock_markets_reconcile_to_app_tickers() {
    let connection = fixture_database();
    let symbols = vec![
        VenueSymbol {
            symbol: "ABC_IDR".into(),
            symbol_type: 2,
            base_asset: "ABC".into(),
            quote_asset: "IDR".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "TSLA_USDT".into(),
            symbol_type: 1,
            base_asset: "TSLA".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "TSLAB_USDT".into(),
            symbol_type: 3,
            base_asset: "TSLAB".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "币安人生_USDT".into(),
            symbol_type: 1,
            base_asset: "币安人生".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
    ];

    let result = reconcile_tokocrypto(&connection, &symbols);
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.inserted, 2);

    let rows = connection
        .prepare(
            "SELECT product_id,symbol,venue_symbol FROM instruments
               WHERE exchange_id='tokocrypto' ORDER BY product_id,symbol",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            ("tokocrypto_spot".into(), "ABC".into(), "ABC_IDR".into(),),
            (
                "tokocrypto_tokenized_stock".into(),
                "TSLA".into(),
                "TSLAB_USDT".into(),
            ),
        ]
    );
}
