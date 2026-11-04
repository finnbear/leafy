//! Support for `#[derive(Properties)]`.

/// Marks a required property as set in a properties builder.
#[doc(hidden)]
pub struct Set;

#[doc(hidden)]
#[diagnostic::on_unimplemented(message = "a required property is missing")]
pub trait IsSet {}
impl IsSet for Set {}
