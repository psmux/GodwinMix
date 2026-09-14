//! The OGraf graphics host.
//!
//! An OGraf graphic is a `graphic.ograf.json` beside a web component: a
//! `schema` saying what words go in it, a `stepCount` saying how many steps
//! `playAction` walks, and a module exporting a custom element with `load`,
//! `updateAction`, `playAction` and `stopAction` on it. The EBU specified it,
//! which is why this host adopts it rather than inventing a template format:
//! the templates already exist.
//!
//! What this plugin does is small on purpose.
//!
//! ```text
//!    tool.call graphic {instance, action, values}
//!            |
//!            v
//!    +-----------------+        GET /graphic/<plugin>/<id>?instance=..
//!    |  state per      |  <---------------------------------------  browser
//!    |  instance       |  --------------------------------------->  source
//!    +-----------------+        GET /events/<instance>  (SSE)
//!            ^
//!            |  the page calls load(), then playAction() when told
//! ```
//!
//! One page per instance, never one page with four graphics on it: a template
//! that throws takes its own page down and not the other three. The state is
//! kept here rather than in the page, so a page that reloads (a browser
//! sidecar restarted by the supervisor) comes back showing what it was
//! showing, which is the difference between a graphics system and a demo.

pub mod catalogue;
pub mod host;
pub mod page;
pub mod state;
