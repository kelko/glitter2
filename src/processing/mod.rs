use std::ops::Index;

use crate::config::model::VariableDefinitionBlock;

mod context;
mod processor;
mod var_store;

pub(crate) use crate::processing::context::ProcessingContext;
pub use crate::processing::processor::GlitterProcessor;

#[derive(Clone, Debug)]
pub(crate) struct ValuePath(Vec<String>);

impl ValuePath {
    #[inline(always)]
    fn append(&mut self, other: &mut ValuePath) {
        self.0.append(&mut other.0);
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline(always)]
    pub(crate) fn drop_first(&mut self) {
        self.0.remove(0);
    }

    #[inline(always)]
    fn render(&self) -> String {
        self.0.join(".")
    }
}

impl Index<usize> for ValuePath {
    type Output = String;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl From<&String> for ValuePath {
    #[inline(always)]
    fn from(path: &String) -> Self {
        ValuePath(
            path.split(".")
                .into_iter()
                .map(|s| s.to_owned())
                .collect::<Vec<_>>(),
        )
    }
}
