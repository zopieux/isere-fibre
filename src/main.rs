mod protos;

use anyhow::{
    Error,
    Result
};
use bytes::Bytes;
use lettre::{
    message::{MultiPart, SinglePart},
    message::header::ContentType,
    message::Mailbox,
    transport::smtp::authentication::Credentials,
    SmtpTransport,
    Transport
};
use maud::html;
use protobuf::Message;
use protos::FeatureCollection::FeatureCollectionPBuffer;
use protos::FeatureCollection::FeatureCollectionPBuffer_Field;
use protos::FeatureCollection::FeatureCollectionPBuffer_Value;
use protos::FeatureCollection::FeatureCollectionPBuffer_Value_oneof_value_type;
use similar::ChangeTag;

fn download_status(url: &str) -> reqwest::Result<bytes::Bytes> {
    reqwest::blocking::get(url)?.bytes()
}

fn as_str(v: &FeatureCollectionPBuffer_Value) -> String {
    match v.value_type.as_ref().unwrap() {
        FeatureCollectionPBuffer_Value_oneof_value_type::string_value(s) => s.to_owned(),
        FeatureCollectionPBuffer_Value_oneof_value_type::float_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::double_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::sint_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::uint_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::int64_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::uint64_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::sint64_value(s) => s.to_string(),
        FeatureCollectionPBuffer_Value_oneof_value_type::bool_value(s) => s.to_string(),
    }
    .trim()
    .to_owned()
}

fn summary_for_address(
    pb: &Bytes,
    addr: &str,
) -> Result<Vec<(FeatureCollectionPBuffer_Field, String)>> {
    let mut query_res = FeatureCollectionPBuffer::parse_from_carllerche_bytes(&pb)?
        .queryResult
        .into_option()
        .ok_or(anyhow::Error::msg("no query result"))?;
    let feature_result = query_res.take_featureResult();
    let addr_idx = feature_result
        .fields
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            if f.name == "CodeAdress" {
                Some(i)
            } else {
                None
            }
        })
        .next()
        .ok_or(Error::msg("no CodeAddress found"))?;
    // Look for the feature having our wanted address.
    let feature = feature_result
        .features
        .into_iter()
        .filter(|feat| feat.attributes[addr_idx].get_string_value() == addr)
        .next()
        .ok_or(Error::msg("no feature found for requested addr"))?;
    // Field-value map.
    Ok(feature
        .attributes
        .into_iter()
        .zip(feature_result.fields.clone())
        // Remove empty attrs.
        .map(|(attr, field)| (field, as_str(&attr)))
        .filter(|(_, attr)| !attr.is_empty())
        .collect())
}

fn generate_summary() -> Result<String> {
    let body = download_status(&std::env::var("URL")?)?;
    let summary = summary_for_address(&body, &std::env::var("ADDRESS")?)?;
    let formatted_summary: Vec<_> = summary
        .iter()
        .map(|(field, attr)| format!("{}/{}: {}", field.name, field.alias, attr))
        .collect();
    Ok(formatted_summary.join("\n"))
}

fn compare_and_notify(old_summary: &str, new_summary: &str) -> Result<()> {
    let diff = similar::TextDiff::from_lines(old_summary, new_summary);
    let diff: Vec<_> = diff
        .unified_diff()
        .iter_hunks()
        .flat_map(|hunk| {
            hunk.iter_changes()
                .map(|change| {
                    let (sign, color) = match change.tag() {
                        ChangeTag::Delete => ("+", "green"),
                        ChangeTag::Insert => ("-", "red"),
                        ChangeTag::Equal => (" ", ""),
                    };
                    html! { strong { (sign) } span style={"color:" (color)} { (change) } }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if diff.is_empty() {
        return Ok(());
    }
    let diff = diff
        .into_iter()
        .fold(html! {}, |acc, g| html! { (acc) (g) });
    let diff: String = html! { pre { code { (diff) } } }
        .into_string()
        .trim()
        .to_owned();
    let addr: Mailbox = std::env::var("GMAIL_ADDRESS")?.parse()?;
    let mail = lettre::Message::builder()
        .from(addr.clone())
        .to(addr)
        .subject("Isère Fibre diff")
        .multipart(
            MultiPart::alternative().singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(diff),
            ),
        )?;
    let creds = Credentials::new(
        std::env::var("GMAIL_USER")?,
        std::env::var("GMAIL_PASSWORD")?,
    );
    let mailer = SmtpTransport::relay("smtp.gmail.com")?
        .credentials(creds)
        .build();
    mailer.send(&mail)?;
    Ok(())
}

fn main() -> Result<()> {
    let path = "last_summary.txt";
    let new_summary = generate_summary()?;
    if let Ok(old_summary) = std::fs::read_to_string(path) {
        compare_and_notify(&old_summary, &new_summary)?;
    }
    std::fs::write(path, new_summary)?;
    Ok(())
}
