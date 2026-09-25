use serde_json::Value;

pub fn parse_did_open(params: &Value) -> Option<(String, String, i64, String)> {
    let document = params.get("textDocument")?;

    let uri = document.get("uri")?.as_str()?.to_owned();

    let language_id = document.get("languageId")?.as_str()?.to_owned();

    let version = document.get("version")?.as_i64()?;

    let text = document.get("text")?.as_str()?.to_owned();

    Some((uri, language_id, version, text))
}

pub fn parse_did_change(params: &Value) -> Option<(String, i64, String)> {
    let document = params.get("textDocument")?;

    let uri = document.get("uri")?.as_str()?.to_owned();

    let version = document.get("version")?.as_i64()?;

    let changes = params.get("contentChanges")?.as_array()?;

    let change = changes.first()?;

    let text = change.get("text")?.as_str()?.to_owned();

    Some((uri, version, text))
}
