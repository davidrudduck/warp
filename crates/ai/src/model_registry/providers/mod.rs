pub mod custom;
pub mod genai_backed;
pub mod openrouter;

#[cfg(test)]
#[path = "openrouter_tests.rs"]
mod openrouter_tests;

#[cfg(test)]
#[path = "custom_tests.rs"]
mod custom_tests;

#[cfg(test)]
#[path = "genai_backed_tests.rs"]
mod genai_backed_tests;
