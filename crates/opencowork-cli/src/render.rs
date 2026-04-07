use opencowork_app::AppEvent;
use opencowork_runtime::{AssistantEvent, ContentBlock, ConversationMessage, RuntimeObserver};
use std::io::{self, Write};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TerminalRenderer;

impl TerminalRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn render_markdown(&self, markdown: &str) -> String {
        markdown.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownStreamState {
    pending: String,
    eager_flush_chars: usize,
}

impl Default for MarkdownStreamState {
    fn default() -> Self {
        Self {
            pending: String::new(),
            eager_flush_chars: 240,
        }
    }
}

impl MarkdownStreamState {
    #[must_use]
    pub fn push(&mut self, renderer: &TerminalRenderer, delta: &str) -> Option<String> {
        self.pending.push_str(delta);
        let split = find_stream_safe_boundary(&self.pending, self.eager_flush_chars)?;
        let ready = self.pending[..split].to_string();
        self.pending.drain(..split);
        Some(renderer.render_markdown(&ready))
    }

    #[must_use]
    pub fn flush(&mut self, renderer: &TerminalRenderer) -> Option<String> {
        if self.pending.trim().is_empty() {
            self.pending.clear();
            None
        } else {
            let pending = std::mem::take(&mut self.pending);
            Some(renderer.render_markdown(&pending))
        }
    }
}

pub struct CliTurnRenderer<'a> {
    out: &'a mut dyn Write,
    renderer: TerminalRenderer,
    markdown_stream: MarkdownStreamState,
    write_error: Option<io::Error>,
    saw_output: bool,
}

impl<'a> CliTurnRenderer<'a> {
    #[must_use]
    pub fn new(out: &'a mut dyn Write) -> Self {
        Self {
            out,
            renderer: TerminalRenderer::new(),
            markdown_stream: MarkdownStreamState::default(),
            write_error: None,
            saw_output: false,
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        self.flush_markdown();
        self.out.flush()?;
        if let Some(error) = self.write_error.take() {
            return Err(error);
        }
        Ok(())
    }

    fn flush_markdown(&mut self) {
        if let Some(rendered) = self.markdown_stream.flush(&self.renderer) {
            self.write_all(rendered.as_bytes());
        }
    }

    fn write_all(&mut self, bytes: &[u8]) {
        if self.write_error.is_some() {
            return;
        }
        self.saw_output = true;
        if let Err(error) = self.out.write_all(bytes).and_then(|()| self.out.flush()) {
            self.write_error = Some(error);
        }
    }

    fn render_tool_result(&self, message: &ConversationMessage) -> Option<String> {
        let ContentBlock::ToolResult {
            tool_name,
            output,
            is_error,
            ..
        } = message.blocks.first()?
        else {
            return None;
        };
        let preview = preview_text(output, 280);
        Some(format!(
            "\n[tool-result:{}] {}\n",
            if *is_error { "error" } else { tool_name },
            preview
        ))
    }

    pub fn on_app_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::AssistantTextDelta { text } => {
                if let Some(rendered) = self.markdown_stream.push(&self.renderer, text) {
                    self.write_all(rendered.as_bytes());
                }
            }
            AppEvent::ToolCall { name, input, .. } => {
                self.flush_markdown();
                self.write_all(format!("\n[tool] {name} {input}\n").as_bytes());
            }
            AppEvent::Usage { usage } => {
                self.write_all(
                    format!(
                        "\n[usage] in={} out={} cache_write={} cache_read={}\n",
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cache_creation_input_tokens,
                        usage.cache_read_input_tokens
                    )
                    .as_bytes(),
                );
            }
            AppEvent::MessageStop => {
                self.flush_markdown();
                self.write_all(b"\n");
            }
            AppEvent::ToolResult {
                tool_name,
                output,
                is_error,
                ..
            } => {
                let preview = preview_text(output, 280);
                self.write_all(
                    format!(
                        "\n[tool-result:{}] {}\n",
                        if *is_error { "error" } else { tool_name },
                        preview
                    )
                    .as_bytes(),
                );
            }
        }
    }
}

impl RuntimeObserver for CliTurnRenderer<'_> {
    fn on_assistant_event(&mut self, event: &AssistantEvent) {
        match event {
            AssistantEvent::TextDelta(text) => {
                if let Some(rendered) = self.markdown_stream.push(&self.renderer, text) {
                    self.write_all(rendered.as_bytes());
                }
            }
            AssistantEvent::ToolUse { name, input, .. } => {
                self.flush_markdown();
                self.write_all(format!("\n[tool] {name} {input}\n").as_bytes());
            }
            AssistantEvent::Usage(usage) => {
                self.write_all(
                    format!(
                        "\n[usage] in={} out={} cache_write={} cache_read={}\n",
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cache_creation_input_tokens,
                        usage.cache_read_input_tokens
                    )
                    .as_bytes(),
                );
            }
            AssistantEvent::MessageStop => {
                self.flush_markdown();
                self.write_all(b"\n");
            }
        }
    }

    fn on_tool_result(&mut self, message: &ConversationMessage) {
        if let Some(rendered) = self.render_tool_result(message) {
            self.write_all(rendered.as_bytes());
        }
    }
}

fn find_stream_safe_boundary(markdown: &str, eager_flush_chars: usize) -> Option<usize> {
    let mut in_fence = false;
    let mut last_boundary = None;
    let mut last_soft_boundary = None;

    for (offset, line) in markdown.split_inclusive('\n').scan(0usize, |cursor, line| {
        let start = *cursor;
        *cursor += line.len();
        Some((start, line))
    }) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            if !in_fence {
                last_boundary = Some(offset + line.len());
            }
            continue;
        }

        if in_fence {
            continue;
        }

        if trimmed.is_empty() {
            last_boundary = Some(offset + line.len());
        } else if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') {
            last_soft_boundary = Some(offset + line.len());
        }
    }

    if let Some(boundary) = last_boundary {
        return Some(boundary);
    }

    if markdown.chars().count() >= eager_flush_chars {
        return last_soft_boundary.or_else(|| {
            markdown
                .char_indices()
                .rev()
                .find(|(_, ch)| ch.is_whitespace())
                .map(|(index, ch)| index + ch.len_utf8())
        });
    }

    None
}

fn preview_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    value.chars().take(limit).collect::<String>() + "..."
}

#[cfg(test)]
mod tests {
    use super::{CliTurnRenderer, MarkdownStreamState, TerminalRenderer};
    use opencowork_runtime::{AssistantEvent, ContentBlock, ConversationMessage, RuntimeObserver};

    #[test]
    fn markdown_stream_waits_for_safe_boundary() {
        let renderer = TerminalRenderer::new();
        let mut stream = MarkdownStreamState::default();
        assert_eq!(stream.push(&renderer, "hello"), None);
        assert_eq!(
            stream.push(&renderer, "\n\nworld"),
            Some("hello\n\n".to_string())
        );
        assert_eq!(stream.flush(&renderer), Some("world".to_string()));
    }

    #[test]
    fn cli_renderer_prints_tool_events() {
        let mut output = Vec::new();
        let mut renderer = CliTurnRenderer::new(&mut output);
        renderer.on_assistant_event(&AssistantEvent::ToolUse {
            id: "1".to_string(),
            name: "read_file".to_string(),
            input: "{\"path\":\"README.md\"}".to_string(),
            required_permission: opencowork_runtime::PermissionMode::ReadOnly,
        });
        renderer.on_tool_result(&ConversationMessage::tool_result(
            "1",
            "read_file",
            "{\"ok\":true}",
            false,
        ));
        renderer.finish().expect("finish");
        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("[tool] read_file"));
        assert!(rendered.contains("[tool-result:read_file]"));
        let _ = ContentBlock::Text {
            text: "demo".to_string(),
        };
    }
}
