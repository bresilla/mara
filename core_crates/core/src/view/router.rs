use crate::workspace::WorkspaceStack;

use super::{MaraView, ViewId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRouterError {
    Empty,
    UnknownView(ViewId),
}

pub struct ViewEntry {
    pub id: ViewId,
    pub title: String,
    pub icon: &'static str,
    pub workspace: WorkspaceStack,
    pub view: Box<dyn MaraView + Send + Sync>,
}

impl ViewEntry {
    #[must_use]
    pub fn new<V>(view: V) -> Self
    where
        V: MaraView + Send + Sync + 'static,
    {
        let id = view.id();
        let title = view.title().to_owned();
        let icon = view.icon();
        assert!(!title.trim().is_empty(), "views require a non-empty title");
        assert!(!icon.trim().is_empty(), "views require a non-empty icon");
        Self {
            id,
            title,
            icon,
            workspace: WorkspaceStack::new(egui::Id::new(("mara_view_workspace", id.0))),
            view: Box::new(view),
        }
    }
}

pub struct ViewRouter {
    active: Option<ViewId>,
    entries: Vec<ViewEntry>,
}

impl ViewRouter {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            active: None,
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn new<V>(view: V) -> Self
    where
        V: MaraView + Send + Sync + 'static,
    {
        let mut router = Self::empty();
        router.register(view);
        router
    }

    pub fn register<V>(&mut self, view: V) -> ViewId
    where
        V: MaraView + Send + Sync + 'static,
    {
        let entry = ViewEntry::new(view);
        let id = entry.id;
        assert!(
            !self.entries.iter().any(|entry| entry.id == id),
            "view routers require unique view ids"
        );
        if self.active.is_none() {
            self.active = Some(id);
        }
        self.entries.push(entry);
        id
    }

    #[must_use]
    pub fn entries(&self) -> &[ViewEntry] {
        &self.entries
    }

    pub fn active(&self) -> Result<ViewId, ViewRouterError> {
        self.active.ok_or(ViewRouterError::Empty)
    }

    pub fn set_active(&mut self, id: ViewId) -> Result<(), ViewRouterError> {
        if self.entries.iter().any(|entry| entry.id == id) {
            self.active = Some(id);
            Ok(())
        } else {
            Err(ViewRouterError::UnknownView(id))
        }
    }

    pub fn active_entry(&self) -> Result<&ViewEntry, ViewRouterError> {
        let id = self.active()?;
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or(ViewRouterError::UnknownView(id))
    }

    pub fn active_entry_mut(&mut self) -> Result<&mut ViewEntry, ViewRouterError> {
        let id = self.active()?;
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(ViewRouterError::UnknownView(id))
    }

    pub fn active_workspace(&self) -> Result<&WorkspaceStack, ViewRouterError> {
        Ok(&self.active_entry()?.workspace)
    }

    pub fn active_workspace_mut(&mut self) -> Result<&mut WorkspaceStack, ViewRouterError> {
        Ok(&mut self.active_entry_mut()?.workspace)
    }
}
