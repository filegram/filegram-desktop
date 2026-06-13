//! The view layer, split by screen. `chrome` holds the shared primitives;
//! `start` is the idle screen; `brick` is the hover actions panel over the
//! map. The scan and map screens still live in `main` and lean on these.

pub(crate) mod brick;
pub(crate) mod chrome;
pub(crate) mod start;
