use anymap::AnyMap;

#[derive(Debug)]
pub struct GlobalVariables {
    map: AnyMap,
}
impl Default for GlobalVariables {
    fn default() -> Self {
        Self { map: AnyMap::new() }
    }
}
impl GlobalVariables {
    pub fn insert<T>(&mut self, value: T) -> Option<T>
    where
        T: 'static,
    {
        self.map.insert(value)
    }
    pub fn get<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        self.map.get::<T>()
    }
    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: 'static,
    {
        self.map.get_mut::<T>()
    }
    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: 'static,
    {
        self.map.remove::<T>()
    }
}
