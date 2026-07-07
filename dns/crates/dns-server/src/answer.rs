use std::time::Instant;
use dns_types::{Handled, HandleError};
use dns_types::zone::Zones;
use dns_types::zone::alias::AliasCache;

pub(crate) fn answer(
    transport: &'static str,
    message: &[u8],
    zones: &Zones,
    aliases: &AliasCache,
) -> Result<Handled, HandleError> {
    let start = Instant::now();

    let handled = match dns_types::handle_message_detailed(message, zones, aliases) {
        Ok(handled) => handled,
        Err(error) => {
            telemetry::metrics().record_error(transport);
            tracing::warn!(transport, error = %error, "failed to parse or handle query");
            return Err(error);
        }
    };

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let rcode = u8::from(handled.rcode);

    let metrics = telemetry::metrics();
    metrics.record_query(transport, handled.qtype);
    metrics.record_response(
        transport,
        rcode,
        handled.answer_kind.as_str(),
        message.len(),
        handled.bytes.len(),
        elapsed_ms,
    );

    let qname = handled.qname.as_ref().map(|name| name.to_display()).unwrap_or_else(|| "<none>".to_string());
    tracing::info!(
        transport,
        qname = %qname,
        qtype = handled.qtype,
        rcode,
        answers = handled.answer_count,
        elapsed_ms,
        "answered query"
    );

    Ok(handled)
}
