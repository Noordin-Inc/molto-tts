use whatlang::Lang;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageMeta {
    pub lang: Lang,
    pub code: &'static str,
    pub alpha2: &'static str,
    pub name: &'static str,
}

pub const ALL_LANGUAGES: &[Lang] = &[
    Lang::Epo, Lang::Eng, Lang::Rus, Lang::Cmn, Lang::Spa,
    Lang::Por, Lang::Ita, Lang::Ben, Lang::Fra, Lang::Deu,
    Lang::Ukr, Lang::Kat, Lang::Ara, Lang::Hin, Lang::Jpn,
    Lang::Heb, Lang::Yid, Lang::Pol, Lang::Amh, Lang::Jav,
    Lang::Kor, Lang::Nob, Lang::Dan, Lang::Swe, Lang::Fin,
    Lang::Tur, Lang::Nld, Lang::Hun, Lang::Ces, Lang::Ell,
    Lang::Bul, Lang::Bel, Lang::Mar, Lang::Kan, Lang::Ron,
    Lang::Slv, Lang::Hrv, Lang::Srp, Lang::Mkd, Lang::Lit,
    Lang::Lav, Lang::Est, Lang::Tam, Lang::Vie, Lang::Urd,
    Lang::Tha, Lang::Guj, Lang::Uzb, Lang::Pan, Lang::Aze,
    Lang::Ind, Lang::Tel, Lang::Pes, Lang::Mal, Lang::Ori,
    Lang::Mya, Lang::Nep, Lang::Sin, Lang::Khm, Lang::Tuk,
    Lang::Aka, Lang::Zul, Lang::Sna, Lang::Afr, Lang::Lat,
    Lang::Slk, Lang::Cat, Lang::Tgl, Lang::Hye,
];

pub fn lang_to_alpha2(lang: Lang) -> &'static str {
    match lang {
        Lang::Epo => "eo",
        Lang::Eng => "en",
        Lang::Rus => "ru",
        Lang::Cmn => "zh",
        Lang::Spa => "es",
        Lang::Por => "pt",
        Lang::Ita => "it",
        Lang::Ben => "bn",
        Lang::Fra => "fr",
        Lang::Deu => "de",
        Lang::Ukr => "uk",
        Lang::Kat => "ka",
        Lang::Ara => "ar",
        Lang::Hin => "hi",
        Lang::Jpn => "ja",
        Lang::Heb => "he",
        Lang::Yid => "yi",
        Lang::Pol => "pl",
        Lang::Amh => "am",
        Lang::Jav => "jv",
        Lang::Kor => "ko",
        Lang::Nob => "nb",
        Lang::Dan => "da",
        Lang::Swe => "sv",
        Lang::Fin => "fi",
        Lang::Tur => "tr",
        Lang::Nld => "nl",
        Lang::Hun => "hu",
        Lang::Ces => "cs",
        Lang::Ell => "el",
        Lang::Bul => "bg",
        Lang::Bel => "be",
        Lang::Mar => "mr",
        Lang::Kan => "kn",
        Lang::Ron => "ro",
        Lang::Slv => "sl",
        Lang::Hrv => "hr",
        Lang::Srp => "sr",
        Lang::Mkd => "mk",
        Lang::Lit => "lt",
        Lang::Lav => "lv",
        Lang::Est => "et",
        Lang::Tam => "ta",
        Lang::Vie => "vi",
        Lang::Urd => "ur",
        Lang::Tha => "th",
        Lang::Guj => "gu",
        Lang::Uzb => "uz",
        Lang::Pan => "pa",
        Lang::Aze => "az",
        Lang::Ind => "id",
        Lang::Tel => "te",
        Lang::Pes => "fa",
        Lang::Mal => "ml",
        Lang::Ori => "or",
        Lang::Mya => "my",
        Lang::Nep => "ne",
        Lang::Sin => "si",
        Lang::Khm => "km",
        Lang::Tuk => "tk",
        Lang::Aka => "ak",
        Lang::Zul => "zu",
        Lang::Sna => "sn",
        Lang::Afr => "af",
        Lang::Lat => "la",
        Lang::Slk => "sk",
        Lang::Cat => "ca",
        Lang::Tgl => "tl",
        Lang::Hye => "hy",
    }
}

pub fn code_to_alpha2(code: &str) -> String {
    if let Some(lang) = find_lang_by_code(code) {
        lang_to_alpha2(lang).to_string()
    } else {
        code.to_lowercase()
    }
}

pub fn get_supported_languages() -> Vec<LanguageMeta> {
    ALL_LANGUAGES
        .iter()
        .map(|&l| LanguageMeta {
            lang: l,
            code: l.code(),
            alpha2: lang_to_alpha2(l),
            name: l.name(),
        })
        .collect()
}

pub fn find_lang_by_code(code: &str) -> Option<Lang> {
    let trimmed = code.trim();
    ALL_LANGUAGES
        .iter()
        .copied()
        .find(|&l| l.code().eq_ignore_ascii_case(trimmed) || lang_to_alpha2(l).eq_ignore_ascii_case(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_languages_count() {
        assert_eq!(ALL_LANGUAGES.len(), 69);
        let list = get_supported_languages();
        assert_eq!(list.len(), 69);
        assert_eq!(find_lang_by_code("eng"), Some(Lang::Eng));
        assert_eq!(find_lang_by_code("en"), Some(Lang::Eng));
        assert_eq!(find_lang_by_code("ara"), Some(Lang::Ara));
        assert_eq!(find_lang_by_code("ar"), Some(Lang::Ara));
        assert_eq!(find_lang_by_code("xyz"), None);
        assert_eq!(code_to_alpha2("eng"), "en");
        assert_eq!(code_to_alpha2("en"), "en");
        assert_eq!(code_to_alpha2("ara"), "ar");
    }
}
