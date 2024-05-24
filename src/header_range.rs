use anyhow::{anyhow, Context};
use hyper::header::HeaderValue;

pub struct HeaderRange {
    pub units: String,
    pub start: u64,
    pub end: u64, // 0 means "until end"
}

#[allow(dead_code)]
impl HeaderRange {
    // This only handles a single range: units=start-end
    pub fn from_header_value(header_value: &HeaderValue) -> anyhow::Result<Self> {
        let s = header_value
            .to_str()
            .context("Failed to convert range header value to string")?;
        if s.contains(',') {
            return Err(anyhow!("Only single ranges can be handled"));
        }
        let equals: Vec<String> = s.split('=').map(|s| s.to_string()).collect();
        if equals.len() != 2 {
            return Err(anyhow!(
                "Failed to parse range header; expected '=' character"
            ));
        }

        let units = equals[0].clone();

        let hyphen: Vec<String> = equals[1].split('-').map(|s| s.to_string()).collect();
        if hyphen.len() != 2 {
            return Err(anyhow!(
                "Failed to parse range header; expected '-' character"
            ));
        }

        let start = if hyphen[0].is_empty() {
            0
        } else {
            hyphen[0]
                .parse::<u64>()
                .context(anyhow!("Invalid range header start value '{}'", hyphen[0]))?
        };

        let end = if hyphen[1].is_empty() {
            0
        } else {
            hyphen[1]
                .parse::<u64>()
                .context(anyhow!("Invalid range header end value '{}'", hyphen[1]))?
        };

        if end != 0 && end < start {
            return Err(anyhow!("Invalid range header; end is before start!"));
        }

        Ok(Self { units, start, end })
    }

    pub fn len(&self) -> usize {
        (self.end - self.start) as usize
    }
}
