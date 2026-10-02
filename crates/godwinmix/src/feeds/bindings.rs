//! `feed.binding.add`, `.set`, `.pause` and `.remove`.
//!
//! A binding added to a feed that already has a document writes at once, so
//! a person sees the value land rather than waiting out the interval.

use super::{bind, check, value, Binding, Ctx, Feeds};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::feeds::{BindingAddRequest, BindingSetRequest, BindingSpec, BindingStatus, BindingTarget};
use std::sync::Arc;

impl Feeds {
    pub async fn bind_add(self: &Arc<Self>, ctx: &Ctx, req: BindingAddRequest) -> Result<BindingStatus, RpcError> {
        self.spec(&req.feed)?;
        let id = match req.id {
            Some(id) => id,
            None => self.free_binding_id(&req.to),
        };
        check::slug("binding", &id)?;
        let spec = BindingSpec {
            id: id.clone(),
            feed: req.feed,
            select: req.select,
            template: req.template.filter(|t| !t.is_empty()),
            limit: req.limit.filter(|n| *n > 0),
            join: req.join,
            to: req.to,
            paused: req.paused,
        };
        self.check_binding(ctx, &spec).await?;
        {
            let mut st = self.state.lock();
            if st.bindings.contains_key(&id) {
                return Err(check::field("id", format!("there is a binding called '{id}' already. Change it with feed.binding.set, or give this one another id.")));
            }
            st.bindings.insert(id.clone(), Binding::new(spec));
            self.save(&st);
        }
        bind::force(self, ctx, &id).await;
        self.binding_status(&id)
    }

    pub async fn bind_set(self: &Arc<Self>, ctx: &Ctx, req: BindingSetRequest) -> Result<BindingStatus, RpcError> {
        let mut spec = self.binding_spec(&req.id)?;
        if let Some(s) = req.select {
            spec.select = s;
        }
        if let Some(t) = req.template {
            spec.template = Some(t).filter(|t| !t.is_empty());
        }
        if let Some(n) = req.limit {
            spec.limit = Some(n).filter(|n| *n > 0);
        }
        if let Some(j) = req.join {
            spec.join = Some(j).filter(|j| !j.is_empty());
        }
        if let Some(to) = req.to {
            spec.to = to;
        }
        self.check_binding(ctx, &spec).await?;
        self.replace_binding(spec);
        bind::force(self, ctx, &req.id).await;
        self.binding_status(&req.id)
    }

    pub async fn bind_pause(self: &Arc<Self>, ctx: &Ctx, id: &str, paused: bool) -> Result<BindingStatus, RpcError> {
        let mut spec = self.binding_spec(id)?;
        spec.paused = paused;
        self.replace_binding(spec);
        if !paused {
            bind::force(self, ctx, id).await;
        }
        self.binding_status(id)
    }

    pub fn bind_remove(&self, id: &str) -> Result<(), RpcError> {
        self.binding_spec(id)?;
        let mut st = self.state.lock();
        st.bindings.remove(id);
        self.save(&st);
        Ok(())
    }

    fn replace_binding(&self, spec: BindingSpec) {
        let mut st = self.state.lock();
        if let Some(b) = st.bindings.get_mut(&spec.id) {
            b.spec = spec;
            b.failures = 0;
            b.last_error = None;
        }
        self.save(&st);
    }

    fn binding_spec(&self, id: &str) -> Result<BindingSpec, RpcError> {
        let st = self.state.lock();
        match st.bindings.get(id) {
            Some(b) => Ok(b.spec.clone()),
            None => Err(RpcError::not_found("binding", id, &st.bindings.keys().cloned().collect::<Vec<_>>())),
        }
    }

    /// The target is a real one, and the selection reads the document the
    /// feed has now. A path that picks nothing is refused here, with the
    /// keys it could have picked, rather than failing quietly on air.
    async fn check_binding(&self, ctx: &Ctx, spec: &BindingSpec) -> Result<(), RpcError> {
        check::target(&spec.to)?;
        if let BindingTarget::Source { source, .. } = &spec.to {
            let status = ctx.app.mixer.status().await.map_err(|e| RpcError::not_in_state(format!("{e:#}")))?;
            let ids: Vec<String> = status.sources.iter().map(|s| s.id.clone()).collect();
            if !ids.contains(source) {
                return Err(RpcError::not_found("source", source, &ids).with("field", "to.source"));
            }
        }
        if let Some(doc) = self.doc(&spec.feed) {
            value::compute(&doc, &spec.selection()).map_err(|e| super::probe::no_value(e, &doc))?;
        }
        Ok(())
    }

    /// `<source>-<param>`, `<field>` or `<scene_param>`, with a number if taken.
    fn free_binding_id(&self, to: &BindingTarget) -> String {
        let base = match to {
            BindingTarget::Source { source, path } => format!("{source}-{}", path.rsplit('.').next().unwrap_or("param")),
            BindingTarget::Graphic { field, .. } => field.clone(),
            BindingTarget::SceneParam { scene_param } => scene_param.clone(),
        };
        let base = crate::channels::keys::slug(&base);
        let st = self.state.lock();
        crate::channels::keys::free(&base, |id| st.bindings.contains_key(id))
    }
}
