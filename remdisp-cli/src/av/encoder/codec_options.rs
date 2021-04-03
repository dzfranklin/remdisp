use std::ffi::{c_void, CStr};
use std::{ptr, fmt};
use ffmpeg_sys_next as av;
use std::os::raw::c_char;
use std::fmt::{Debug, Formatter};
use crate::av::ensure_av_logs_setup;

pub struct Options {
    children: Vec<Option>
}

pub enum Option {
    Entry {
        name: String,
    },
    Dict {
        name: String,
        value: Box<Options>,
    },
}

impl Options {
    pub unsafe fn from(obj: *const c_void) -> Options {
        ensure_av_logs_setup();

        let mut out = Options::default();

        // See <https://ffmpeg.org/doxygen/3.4/group__lavu__dict.html#gae67f143237b2cb2936c9b147aa6dfde3>
        let mut prev = ptr::null();
        loop {
            let entry = av::av_opt_next(obj, prev);
            if let Some(entry) = ptr::NonNull::new(entry as *mut _) {
                let entry_ref: &av::AVOption = entry.as_ref();
                let k: *const c_char = entry_ref.name;

                // Safety: We just assume ffmpeg produces sane values
                let name = CStr::from_ptr(k).to_string_lossy().into_owned();

                if entry_ref.type_ == av::AVOptionType::AV_OPT_TYPE_DICT {
                    let child_data = obj.add(entry_ref.offset as usize);
                    let value = Self::from(child_data);
                    out.children.push(Self::new_dict(name, value));
                } else {
                    out.children.push(Self::new_entry(name));
                }

                prev = entry.as_ptr();
            } else {
                break;
            }
        }

        out
    }

    fn new_entry(name: String) -> Option {
        Option::Entry { name }
    }

    fn new_dict(name: String, dict: Options) -> Option {
        Option::Dict {
            name: name,
            value: Box::new(dict)
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            children: vec![],
        }
    }
}

impl Debug for Options {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(&self.children)
            .finish()
    }
}

impl Debug for Option {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Option::Entry { name } =>
                write!(f, "{}", name),
            Option::Dict { name, value } =>
                write!(f, "{}={:?}", name, value)
        }
    }
}
