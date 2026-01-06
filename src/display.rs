#[cfg(feature = "ili9488")]
pub mod ili9488;
#[cfg(feature = "ili9488")]
pub use ili9488::*;

#[cfg(feature = "st7920")]
pub mod st7920;
#[cfg(feature = "st7920")]
pub use st7920::*;

#[cfg(all(feature = "ili9488", feature = "st7920"))]
compile_error!("Features 'ili9488' and 'st7920' are mutually exclusive.");

#[cfg(not(any(feature = "ili9488", feature = "st7920")))]
compile_error!("One of 'ili9488' or 'st7920' feature must be enabled.");
