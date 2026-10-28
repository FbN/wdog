pub mod logfmt;
pub mod ring;
pub mod splitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}
