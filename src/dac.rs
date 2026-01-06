pub mod common;

#[cfg(feature = "ak4490")]
pub mod ak4490;
#[cfg(feature = "ak4497")]
pub mod ak4497;

#[cfg(all(feature = "ak4490", feature = "ak4497"))]
compile_error!("Features 'ak4490' and 'ak4497' are mutually exclusive.");

#[cfg(not(any(feature = "ak4490", feature = "ak4497")))]
compile_error!("One of 'ak4490' or 'ak4497' feature must be enabled.");
