use crate::RenderEngine;
use std::sync::Arc;

pub trait ResourceDrop {
    fn destroy(&mut self, engine: &RenderEngine);
}

pub struct Resource<T: ResourceDrop + 'static> {
    handle: Option<T>,
    pub engine: Arc<RenderEngine>,
}

impl<T: ResourceDrop + 'static> Resource<T> {
    pub(crate) fn make(engine: Arc<RenderEngine>, handle: T) -> Self {
        Resource {
            handle: Some(handle),
            engine,
        }
    }

    pub (crate) fn get_handle(&self) -> &T {
        self.handle.as_ref().unwrap()
    }
}

impl<T: ResourceDrop + 'static> Drop for Resource<T> {
    fn drop(&mut self) {
        let handle = self.handle.take().unwrap();
        self.engine.schedule_resource_release(handle);
    }
}
