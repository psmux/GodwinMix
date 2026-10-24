//! Reading the config `headend.import` was given: pasted text, or a file
//! on the station's machine, parsed by `godwinmix_flussonic`. A mistake is
//! refused with its line, as Flussonic would refuse the file.

use godwinmix_flussonic::Document;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::HeadendImportRequest;

/// The largest config read, in bytes. A 200 stream config is about 60 kB.
const MOST: u64 = 4 << 20;

/// The config's text and where it came from.
pub fn source(req: &HeadendImportRequest) -> Result<(String, String), RpcError> {
    match (req.text.as_deref(), req.path.as_deref()) {
        (Some(t), None) => Ok(("pasted text".into(), t.to_string())),
        (None, Some(p)) => {
            let len = std::fs::metadata(p).map(|m| m.len()).map_err(|e| unreadable(p, &e))?;
            if len > MOST {
                return Err(RpcError::invalid_params(format!("{p} is {len} bytes, more than a Flussonic config is. Check the path names the config.")).with("path", p));
            }
            Ok((p.to_string(), std::fs::read_to_string(p).map_err(|e| unreadable(p, &e))?))
        }
        _ => Err(RpcError::invalid_params("headend.import takes the config as text, or a path on the station's machine: send one of the two.")),
    }
}

fn unreadable(path: &str, e: &std::io::Error) -> RpcError {
    RpcError::invalid_params(format!("the station could not read {path}: {e}. Give a path on the station's machine that it may read, or paste the text.")).with("path", path)
}

pub fn parse(from: &str, text: &str) -> Result<Document, RpcError> {
    Document::parse(text).map_err(|e| {
        let line = match &e {
            godwinmix_flussonic::Error::Syntax { line, .. } | godwinmix_flussonic::Error::Invalid { line, .. } => *line,
            godwinmix_flussonic::Error::Edit(_) => 0,
        };
        RpcError::invalid_params(format!("{from} is not a config Flussonic would read, {e}. Fix that line and import again.")).with("line", line)
    })
}

