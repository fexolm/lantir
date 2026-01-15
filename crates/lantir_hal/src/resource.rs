use crate::RenderEngine;
use std::ops::Deref;
use std::sync::Arc;

pub trait DeferDrop {
    fn destroy(&mut self, engine: &RenderEngine);
}

pub struct Resource<T: DeferDrop + 'static> {
    handle: Option<T>,
    pub engine: Arc<RenderEngine>,
}

impl<T: DeferDrop + 'static> Resource<T> {
    pub(crate) fn make(engine: Arc<RenderEngine>, handle: T) -> Self {
        Resource {
            handle: Some(handle),
            engine,
        }
    }

    pub(crate) fn get_handle(&self) -> &T {
        self.handle.as_ref().unwrap()
    }
}

impl<T: DeferDrop + 'static> Deref for Resource<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get_handle()
    }
}

impl<T: DeferDrop + 'static> Drop for Resource<T> {
    fn drop(&mut self) {
        let handle = self.handle.take().unwrap();
        self.engine.schedule_resource_release(handle);
    }
}
