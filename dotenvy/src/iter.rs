use crate::{
    err::{Error, Result},
    parse, EnvMap,
};
use std::{
    collections::HashMap,
    env::{self},
    io::{self, BufRead},
    result::Result as StdResult,
};

pub struct Iter<B> {
    lines: Lines<B>,
    substitution_data: HashMap<String, Option<String>>,
}

impl<B: BufRead> Iter<B> {
    pub fn new(buf: B) -> Self {
        Self {
            lines: Lines(buf),
            substitution_data: HashMap::new(),
        }
    }

    fn internal_load<F>(mut self, mut load_fn: F) -> Result<EnvMap>
    where
        F: FnMut(String, String, &mut EnvMap),
    {
        self.remove_bom()?;
        let mut map = EnvMap::new();
        for item in self {
            let (k, v) = item?;
            load_fn(k, v, &mut map);
        }
        Ok(map)
    }

    pub fn load(self) -> Result<EnvMap> {
        self.internal_load(|k, v, map| {
            map.insert(k, v);
        })
    }

    pub unsafe fn load_and_modify(self) -> Result<EnvMap> {
        self.internal_load(|k, v, map| {
            if env::var(&k).is_err() {
                unsafe { env::set_var(&k, &v) };
            }
            map.insert(k, v);
        })
    }

    pub unsafe fn load_and_modify_override(self) -> Result<EnvMap> {
        self.internal_load(|k, v, map| {
            unsafe { env::set_var(&k, &v) };
            map.insert(k, v);
        })
    }

    /// Removes the BOM from the reader if it exists.
    ///
    /// For more details, see the [Unicode BOM character](https://www.compart.com/en/unicode/U+FEFF).
    fn remove_bom(&mut self) -> StdResult<(), io::Error> {
        let buf = self.lines.0.fill_buf()?;

        if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.lines.0.consume(3);
        }
        Ok(())
    }
}

struct Lines<B>(B);

enum ParseState {
    Complete,
    Escape,
    StrongOpen,
    StrongOpenEscape,
    WeakOpen,
    WeakOpenEscape,
    Comment,
    WhiteSpace,
}

impl ParseState {
    fn eval_end(self, buf: &str) -> (usize, Self) {
        let mut cur_state = self;
        let mut cur_pos = 0;

        for (pos, c) in buf.char_indices() {
            cur_pos = pos;
            cur_state = match cur_state {
                Self::WhiteSpace => match c {
                    '#' => return (cur_pos, Self::Comment),
                    '\\' => Self::Escape,
                    '"' => Self::WeakOpen,
                    '\'' => Self::StrongOpen,
                    _ => Self::Complete,
                },
                Self::Escape => Self::Complete,
                Self::Complete => match c {
                    c if c.is_whitespace() && c != '\n' && c != '\r' => Self::WhiteSpace,
                    '\\' => Self::Escape,
                    '"' => Self::WeakOpen,
                    '\'' => Self::StrongOpen,
                    _ => Self::Complete,
                },
                Self::WeakOpen => match c {
                    '\\' => Self::WeakOpenEscape,
                    '"' => Self::Complete,
                    _ => Self::WeakOpen,
                },
                Self::WeakOpenEscape => Self::WeakOpen,
                Self::StrongOpen => match c {
                    '\\' => Self::StrongOpenEscape,
                    '\'' => Self::Complete,
                    _ => Self::StrongOpen,
                },
                Self::StrongOpenEscape => Self::StrongOpen,
                // Comments last the entire line.
                Self::Comment => unreachable!("should have returned already"),
            };
        }
        (cur_pos, cur_state)
    }
}

impl<B: BufRead> Iterator for Lines<B> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Result<String>> {
        let mut buf = String::new();
        let mut cur_state = ParseState::Complete;
        let mut buf_pos;
        let mut cur_pos;
        loop {
            buf_pos = buf.len();
            match self.0.read_line(&mut buf) {
                Ok(0) => {
                    if matches!(cur_state, ParseState::Complete) {
                        return None;
                    }
                    let len = buf.len();
                    return Some(Err(Error::LineParse(buf, len)));
                }
                Ok(_n) => {
                    // Skip lines which start with a `#` before iteration
                    // This optimizes parsing a bit.
                    if buf.trim_start().starts_with('#') {
                        return Some(Ok(String::with_capacity(0)));
                    }
                    let result = cur_state.eval_end(&buf[buf_pos..]);
                    cur_pos = result.0;
                    cur_state = result.1;

                    match cur_state {
                        ParseState::Complete => {
                            if buf.ends_with('\n') {
                                buf.pop();
                                if buf.ends_with('\r') {
                                    buf.pop();
                                }
                            }
                            return Some(Ok(buf));
                        }
                        ParseState::Escape
                        | ParseState::StrongOpen
                        | ParseState::StrongOpenEscape
                        | ParseState::WeakOpen
                        | ParseState::WeakOpenEscape
                        | ParseState::WhiteSpace => {}
                        ParseState::Comment => {
                            buf.truncate(buf_pos + cur_pos);
                            return Some(Ok(buf));
                        }
                    }
                }
                Err(e) => return Some(Err(Error::Io(e))),
            }
        }
    }
}

impl<B: BufRead> Iterator for Iter<B> {
    type Item = Result<(String, String)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line = match self.lines.next() {
                Some(Ok(line)) => line,
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            };

            match parse::parse_line(&line, &mut self.substitution_data) {
                Ok(Some(res)) => return Some(Ok(res)),
                Ok(None) => {}
                Err(e) => return Some(Err(e)),
            }
        }
    }
}
