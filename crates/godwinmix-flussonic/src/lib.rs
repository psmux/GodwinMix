//! Flussonic compatibility for GodwinMix headend mode.
//!
//! `Document` holds the file as written (`syntax`) and the typed view
//! (`model`). Edits go through the syntax tree, so whatever GodwinMix does
//! not understand survives a save. `import` turns the view into direct
//! shows, `diff` says which of them a changed file restarts, and `facade`,
//! `api` and `log` are the shapes Flussonic's readers expect: its URLs and
//! playlists, its API's JSON, its log lines. No media and no server here;
//! the binary serves them (`godwinmix headend`).

pub mod api;
pub mod clock;
pub mod diff;
pub mod facade;
pub mod import;
pub mod log;
pub mod model;
pub mod syntax;

pub use model::*;
pub use syntax::{Directive, Node};

#[derive(Debug, PartialEq)]
pub enum Error {
    Syntax { line: usize, msg: String },
    Invalid { line: usize, msg: String },
    Edit(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Syntax { line, msg } | Error::Invalid { line, msg } => write!(f, "line {line}: {msg}"),
            Error::Edit(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
pub struct Document {
    pub nodes: Vec<Node>,
    pub config: Config,
}

impl Document {
    pub fn parse(src: &str) -> Result<Self, Error> {
        let nodes = syntax::parse(src)?;
        let config = model::from_nodes(&nodes)?;
        Ok(Document { nodes, config })
    }

    pub fn text(&self) -> String {
        syntax::print(&self.nodes)
    }

    fn reload(&mut self) -> Result<(), Error> {
        self.config = model::from_nodes(&self.nodes)?;
        Ok(())
    }

    fn stream_mut(&mut self, name: &str) -> Option<&mut Directive> {
        self.nodes.iter_mut().find_map(|n| match n {
            Node::Directive(d) if d.name == "stream" && d.arg(0) == Some(name) => Some(d),
            _ => None,
        })
    }

    /// Turn a stream on or off, the way Flussonic's admin does: a
    /// `disabled` line inside the block.
    pub fn set_disabled(&mut self, name: &str, disabled: bool) -> Result<(), Error> {
        let d = self.stream_mut(name).ok_or_else(|| Error::Edit(format!("no stream {name}")))?;
        let value = if disabled { "true" } else { "false" };
        let body = d.block.get_or_insert_with(Vec::new);
        match body.iter_mut().find_map(|n| match n {
            Node::Directive(c) if c.name == "disabled" => Some(c),
            _ => None,
        }) {
            Some(c) => c.args = vec![value.into()],
            None => body.insert(0, Node::Directive(Directive::new("disabled", vec![value.into()]))),
        }
        self.reload()
    }

    /// Replace a stream's block with new text (`stream NAME { ... }`), or add
    /// it after the last stream. The text is checked before anything changes.
    pub fn put_stream(&mut self, name: &str, text: &str) -> Result<(), Error> {
        let parsed = syntax::parse(text)?;
        let mut blocks = parsed.into_iter().filter_map(|n| match n {
            Node::Directive(d) => Some(d),
            _ => None,
        });
        let block = blocks.next().ok_or_else(|| Error::Edit("no stream block in the text".into()))?;
        if blocks.next().is_some() || block.name != "stream" || block.arg(0) != Some(name) {
            return Err(Error::Edit(format!("the text must be exactly one 'stream {name} {{ ... }}' block")));
        }
        model::from_nodes(&[Node::Directive(block.clone())])?;
        match self.stream_mut(name) {
            Some(d) => *d = block,
            None => {
                let at = self
                    .nodes
                    .iter()
                    .rposition(|n| matches!(n, Node::Directive(d) if d.name == "stream"))
                    .map(|i| i + 1)
                    .unwrap_or(self.nodes.len());
                self.nodes.insert(at, Node::Directive(block));
            }
        }
        self.reload()
    }

    pub fn remove_stream(&mut self, name: &str) -> Result<(), Error> {
        let before = self.nodes.len();
        self.nodes.retain(|n| !matches!(n, Node::Directive(d) if d.name == "stream" && d.arg(0) == Some(name)));
        if self.nodes.len() == before {
            return Err(Error::Edit(format!("no stream {name}")));
        }
        self.reload()
    }

    /// The text of one stream's block, for the dashboard's editor.
    pub fn stream_text(&self, name: &str) -> Option<String> {
        self.nodes.iter().find_map(|n| match n {
            Node::Directive(d) if d.name == "stream" && d.arg(0) == Some(name) => {
                Some(syntax::print(std::slice::from_ref(n)))
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/sample-headend.conf");

    #[test]
    fn reads_a_headend_config() {
        let doc = Document::parse(SAMPLE).unwrap();
        let c = &doc.config;
        assert_eq!(c.listeners, vec![Listen { addr: None, port: 8090 }]);
        assert_eq!(c.edit_auth.as_ref().unwrap().user, "admin");
        assert_eq!(c.streams.len(), 52);
        assert_eq!(c.dvrs[0].root.as_deref(), Some("/var/lib/dvr"));
        let transcoded = c.streams.iter().filter(|s| s.transcoder.is_some()).count();
        assert_eq!(transcoded, 51);
        let relay = c.streams.iter().find(|s| s.name == "RELAY1").unwrap();
        assert!(relay.transcoder.is_none());
        assert_eq!(relay.inputs[0].source, Source::Url { scheme: "rtmp".into() });
        assert!(c.unknown.is_empty(), "{:?}", c.unknown);
        // 8 multicast groups feed 51 streams.
        let mut groups: Vec<_> = c
            .streams
            .iter()
            .filter_map(|s| match &s.inputs[0].source {
                Source::Udp { port, .. } => Some(*port),
                _ => None,
            })
            .collect();
        groups.sort();
        groups.dedup();
        assert_eq!(groups.len(), 8);
    }

    #[test]
    fn writes_back_what_it_read() {
        let doc = Document::parse(SAMPLE).unwrap();
        let again = Document::parse(&doc.text()).unwrap();
        assert_eq!(again.nodes, doc.nodes);
        assert_eq!(again.config, doc.config);
    }

    #[test]
    fn edits_keep_everything_else() {
        let mut doc = Document::parse("http 8090;\nfoo bar;\nstream A {\n  input udp://239.0.0.1:1;\n  weird 1;\n}\n").unwrap();
        doc.set_disabled("A", true).unwrap();
        assert!(doc.config.streams[0].disabled);
        assert_eq!(doc.text(), "http 8090;\nfoo bar;\nstream A {\n  disabled true;\n  input udp://239.0.0.1:1;\n  weird 1;\n}\n");
        doc.put_stream("B", "stream B { input rtmp://x/y; }").unwrap();
        assert_eq!(doc.config.streams[1].name, "B");
        assert!(doc.put_stream("C", "stream D { }").is_err());
        assert!(doc.put_stream("C", "stream C { input nope; }").is_err());
        doc.remove_stream("A").unwrap();
        assert_eq!(doc.config.streams.len(), 1);
        assert!(doc.text().contains("foo bar;"));
    }
}
