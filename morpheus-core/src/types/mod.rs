pub mod analysis;
pub mod dialect;
pub mod morph_flags;
pub mod stem_type;
pub mod word_form;

pub use analysis::{Analysis, GkString};
pub use dialect::Dialect;
pub use morph_flags::MorphFlags;
pub use stem_type::StemType;
pub use word_form::WordForm;
