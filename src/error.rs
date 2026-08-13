use std::{collections::BTreeMap, fmt, io, sync::Arc};

#[derive(Debug, Clone)]
pub struct MdlError {
    pub key: &'static str,
    pub args: BTreeMap<&'static str, String>,
    pub causes: Vec<MdlCause>,
}

#[derive(Debug, Clone)]
pub enum MdlCause {
    Mdl(Box<MdlError>),
    Std(Arc<dyn std::error::Error + Send + Sync>),
}

impl MdlError {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            args: BTreeMap::new(),
            causes: Vec::new(),
        }
    }

    pub fn with_arg(mut self, k: &'static str, v: impl ToString) -> Self {
        self.args.insert(k, v.to_string());
        self
    }

    #[allow(dead_code)]
    pub fn with_args(mut self, args: impl IntoIterator<Item = (&'static str, String)>) -> Self {
        for (k, v) in args {
            self.args.insert(k, v);
        }
        self
    }

    #[allow(dead_code)]
    pub fn push_mdl(mut self, cause: MdlError) -> Self {
        self.causes.push(MdlCause::Mdl(Box::new(cause)));
        self
    }

    pub fn push_std(mut self, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.causes.push(MdlCause::Std(Arc::new(cause)));
        self
    }
}

impl fmt::Display for MdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.key)?;
        let mut first = true;
        for (k, v) in &self.args {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            write!(f, "{k}={v}")?;
        }
        write!(f, ")")
    }
}

impl std::error::Error for MdlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.causes.iter().find_map(|c| match c {
            MdlCause::Mdl(e) => Some(e.as_ref() as &dyn std::error::Error),
            MdlCause::Std(e) => Some(e.as_ref()),
        })
    }
}

impl From<String> for MdlError {
    fn from(s: String) -> Self {
        MdlError::new("string-error").with_arg("msg", s)
    }
}

impl From<&str> for MdlError {
    fn from(s: &str) -> Self {
        MdlError::new("str-error").with_arg("msg", s)
    }
}

impl From<reqwest::Error> for MdlError {
    fn from(err: reqwest::Error) -> Self {
        MdlError::new("reqwest::Error").push_std(err)
    }
}

impl From<io::Error> for MdlError {
    fn from(err: io::Error) -> Self {
        MdlError::new("io-error").push_std(err)
    }
}

impl From<blp::error::error::BlpError> for MdlError {
    fn from(err: blp::error::error::BlpError) -> Self {
        MdlError::new("blp-error").push_std(err)
    }
}

impl From<wgpu::CreateSurfaceError> for MdlError {
    fn from(err: wgpu::CreateSurfaceError) -> Self {
        MdlError::new("wgpu::CreateSurfaceError").push_std(err)
    }
}

impl From<winit::error::EventLoopError> for MdlError {
    fn from(err: winit::error::EventLoopError) -> Self {
        MdlError::new("winit::error::EventLoopError").push_std(err)
    }
}
