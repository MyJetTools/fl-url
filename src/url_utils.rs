use std::collections::HashMap;

use lazy_static::lazy_static;

pub fn parse_query_string(result: &mut HashMap<String, String>, query_string: &str) {
    let elements = query_string.split("&");

    for el in elements {
        let kv = el.find('=');

        if let Some(index) = kv {
            let key = decode_from_url_string(&el[..index]);
            let value = decode_from_url_string(&el[index + 1..]);
            result.insert(key, value);
        }
    }
}

pub fn decode_from_url_string(src: &str) -> String {
    let index = src.find("%");

    if index.is_none() {
        return src.to_string();
    }

    let mut result: Vec<u8> = Vec::new();

    let mut is_escape_symbol_mode = false;
    let mut escape_pos: u32 = 0;
    let mut escape0: u8 = 0;

    for (_, c) in src.chars().enumerate() {
        if is_escape_symbol_mode {
            if escape_pos == 0 {
                escape0 = c as u8;
                escape_pos += 1;
            } else if escape_pos == 0 {
                escape_pos += 1;
                let c = decode_url_escape(escape0, c as u8);
                result.push(c);
                is_escape_symbol_mode = false;
            }
        } else {
            if c != '%' {
                if c == '+' {
                    result.push(' ' as u8);
                } else {
                    result.push(c as u8);
                }
            } else {
                is_escape_symbol_mode = true;
                escape_pos = 0;
            }
        }
    }

    return String::from_utf8(result).unwrap();
}

pub fn decode_url_escape(s0: u8, s1: u8) -> u8 {
    if s0 == b'2' {
        return URL_DECODE_SYMBOLS_2.get(&s1).unwrap().clone();
    }

    if s0 == b'3' {
        return URL_DECODE_SYMBOLS_3.get(&s1).unwrap().clone();
    }

    if s0 == b'4' && s1 == b'0' {
        return b'@';
    }

    if s0 == b'5' {
        if s1 == b'B' || s1 == b'b' {
            return b'[';
        }
        if s1 == b'D' || s1 == b'D' {
            return b']';
        }
    }

    panic!("Invalid URL Symbol %{}{}", s0 as char, s1 as char);
}

pub fn encode_to_url_string_and_copy(res: &mut String, src: &str) {
    let mut has_symbol_to_encode = false;
    for (_, c) in src.chars().enumerate() {
        if URL_ENCODE_SYMBOLS.contains_key(&c) {
            has_symbol_to_encode = true;
            break;
        }
    }

    if !has_symbol_to_encode {
        res.push_str(src);
        return;
    }

    for (_, c) in src.chars().enumerate() {
        let found = URL_ENCODE_SYMBOLS.get(&c);

        match found {
            Some(str) => {
                res.push_str(str);
            }
            None => {
                res.push(c);
            }
        }
    }
}

lazy_static! {
    static ref URL_ENCODE_SYMBOLS: HashMap<char, &'static str> = [
        (' ', "+"),
        ('#', "%23"),
        ('$', "%24"),
        ('%', "%25"),
        ('&', "%26"),
        ('\'', "%27"),
        ('(', "%28"),
        (')', "%29"),
        ('*', "%2A"),
        ('+', "%2B"),
        (',', "%2C"),
        ('/', "%2F"),
        (':', "%3A"),
        (';', "%3B"),
        ('=', "%3D"),
        ('?', "%3F"),
        ('@', "%40"),
        ('[', "%5B"),
        (']', "%5D"),
    ]
    .iter()
    .copied()
    .collect();
}

lazy_static! {
    static ref URL_DECODE_SYMBOLS_2: HashMap<u8, u8> = [
        (b'3', b'#'),
        (b'4', b'$'),
        (b'5', b'%'),
        (b'6', b'&'),
        (b'7', b'\''),
        (b'8', b'('),
        (b'9', b')'),
        (b'A', b'*'),
        (b'a', b'*'),
        (b'B', b'+'),
        (b'b', b'+'),
        (b'C', b','),
        (b'c', b','),
        (b'F', b'/'),
        (b'f', b'/'),
    ]
    .iter()
    .copied()
    .collect();
}

lazy_static! {
    static ref URL_DECODE_SYMBOLS_3: HashMap<u8, u8> = [
        (b'A', b':'),
        (b'a', b':'),
        (b'B', b';'),
        (b'b', b';'),
        (b'D', b'='),
        (b'd', b'='),
        (b'F', b'?'),
        (b'f', b'?'),
//        ('@', "%40"),
//        ('[', "%5B"),
//        (']', "%5D"),
    ]
    .iter()
    .copied()
    .collect();
}
