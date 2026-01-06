use crate::RenderEngine;
use std::ops::Deref;
use std::sync::Arc;

pub trait ResourceDrop {
    fn destroy(&mut self, engine: &RenderEngine);
}

pub struct Resource<T: ResourceDrop + 'static> {
    handle: Option<T>,
    engine: Arc<RenderEngine>,
}

impl<T: ResourceDrop + 'static> Resource<T> {
    pub fn new(engine: Arc<RenderEngine>, handle: T) -> Self {
        Resource {
            handle: Some(handle),
            engine,
        }
    }

    pub fn get_handle(&self) -> &T {
        &self.handle.as_ref().unwrap()
    }
}

impl<T: ResourceDrop + 'static> Drop for Resource<T> {
    fn drop(&mut self) {
        let handle = self.handle.take().unwrap();
        self.engine.schedule_resource_release(handle);
    }
}

impl<T: ResourceDrop> Deref for Resource<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.handle.as_ref().unwrap()
    }
}
