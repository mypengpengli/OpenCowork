use super::*;
#[derive(Deserialize)]
pub(super) struct Search {
    q: String,
    #[serde(default)]
    all: bool,
}
fn excerpt(text: &str, query: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let position = lower.find(query)?;
    // Lowercasing can change UTF-8 byte lengths; positions are used only in the
    // lowercased string, and slicing is by Unicode scalar values.
    let count = lower[..position].chars().count();
    Some(
        text.chars()
            .skip(count.saturating_sub(60))
            .take(240)
            .collect(),
    )
}
pub(super) async fn search(
    State(state): State<ShellState>,
    Query(q): Query<Search>,
) -> Result<Json<Value>, ApiError> {
    let query = q.q.trim().to_lowercase();
    if query.is_empty() || query.len() > 512 {
        return Err(ApiError::bad_request("Search needs 1–512 bytes"));
    }
    tokio::task::spawn_blocking(move||{
        let root=state.config_home.join("sessions");let index=root.join(".search-index");
        let _lease=opencowork_runtime::ExclusiveLease::acquire(&index.join("index.lock")).map_err(internal_error)?;
        let mut results=Vec::new();let mut present=std::collections::BTreeSet::new();
        let mut entries=fs::read_dir(&root).map_err(internal_error)?.flatten().filter(|e|e.path().extension().is_some_and(|x|x=="json")).collect::<Vec<_>>();
        entries.sort_by_key(|e|std::cmp::Reverse(e.metadata().ok().and_then(|m|m.modified().ok())));
        for entry in entries {
            let id=entry.path().file_stem().unwrap().to_string_lossy().into_owned();if !workflows::valid(&id){continue;}present.insert(id.clone());
            let meta=entry.metadata().map_err(internal_error)?;
            let stamp=format!("{}-{:?}",meta.len(),meta.modified().ok());let cached=index.join(format!("{id}.json"));
            let mut value:Value=fs::read(&cached).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
            if value["stamp"]!=stamp {
                let session=Session::load_from_path(entry.path()).map_err(internal_error)?;
                let mut messages=Vec::new();
                for (archived,source) in [(false,&session.messages),(true,&session.context_collapse_archive)] {
                    for (i,m) in source.iter().enumerate(){
                        for b in &m.blocks {let text=match b {opencowork_runtime::ContentBlock::Text{text}=>text,opencowork_runtime::ContentBlock::ToolResult{output,..}=>output,_=>continue};
                            messages.push(serde_json::json!({"index":i,"archived":archived,"text":text}));
                        }
                    }
                }
                value=serde_json::json!({"stamp":stamp,"workspace":session.workspace,"messages":messages});
                opencowork_runtime::write_json_atomic(&cached,&value).map_err(internal_error)?;
            }
            if !q.all && value["workspace"].as_str()!=Some(state.cwd.to_string_lossy().as_ref()){continue;}
            if results.len()>=100 {continue;}
            if let Some(messages)=value["messages"].as_array(){for m in messages {
                if let Some(snippet)=m["text"].as_str().and_then(|s|excerpt(s,&query)) {results.push(serde_json::json!({"sessionId":id,"workspace":value["workspace"],"messageIndex":m["index"],"archived":m["archived"],"snippet":snippet}));if results.len()>=100{break;}}
            }}
        }
        // Removed sessions must not survive in the search cache.
        for entry in fs::read_dir(&index).map_err(internal_error)?.flatten(){let p=entry.path();if p.extension().is_some_and(|e|e=="json") && p.file_stem().is_some_and(|s|!present.contains(s.to_string_lossy().as_ref())){fs::remove_file(p).map_err(internal_error)?;}}
        Ok(Json(serde_json::json!({"results":results,"limit":100})))
    }).await.map_err(internal_error)?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chinese_and_unicode() {
        assert!(excerpt("之前完成中文桌面控制，现在验证截图", "桌面控制")
            .unwrap()
            .contains("桌面控制"));
        assert!(excerpt("Hello World", "world").is_some());
        assert!(excerpt("旧内容", "新内容").is_none());
    }
}
