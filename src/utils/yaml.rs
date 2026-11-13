//! YAML parsing options that preserve strings accepted by the previous parser.

pub(crate) fn options() -> serde_saphyr::Options {
    serde_saphyr::options! {
        leading_zero_numbers_as_strings: true,
        underscored_numbers_as_strings: true,
    }
}
