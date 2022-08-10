use symbolic::smcache::{ScopeLookupResult, SmCache, SmCacheWriter, SourcePosition};
use tracing_subscriber::fmt;

fn main() {
    fmt()
        .with_span_events(fmt::format::FmtSpan::NEW | fmt::format::FmtSpan::CLOSE)
        .event_format(tracing_subscriber::fmt::format().compact().without_time())
        .with_max_level(tracing::Level::TRACE)
        .init();

    let minified = std::fs::read_to_string("fixtures/sentry.js").unwrap();
    let sourcemap = std::fs::read_to_string("fixtures/sentry.js.map").unwrap();

    let writer = SmCacheWriter::new(&minified, &sourcemap).unwrap();
    let mut buffer = Vec::new();
    writer.serialize(&mut buffer).unwrap();
    let cache = SmCache::parse(&buffer).unwrap();
    let sp = SourcePosition::new(0, 51238);
    let token = cache.lookup(sp).unwrap();

    assert_eq!(token.line(), 84);
    assert_eq!(
        token.scope(),
        ScopeLookupResult::NamedScope("sentryWrapped")
    );
    assert_eq!(
        token.file().unwrap().name(),
        "../node_modules/@sentry/browser/esm/helpers.js"
    );
}
