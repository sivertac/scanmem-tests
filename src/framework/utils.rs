

pub fn compute_median<I>(values: I) -> f64 where I: Iterator<Item = f64>, {
    let mut data: Vec<f64> = values.collect();
    data.sort_by(|a,b|a.total_cmp(b));
    return data[data.len() / 2];
}

pub fn compute_standard_deviation<I>(values: I, mean: f64) -> f64 where I: Iterator<Item = f64>, {
    let data: Vec<f64> = values.collect();
    let len = data.len();
    let sum = data.into_iter().reduce(|acc: f64, e: f64| acc + (e - mean)).unwrap();
    return f64::sqrt(1.0f64 / len as f64 * sum.powi(2));
}

pub fn internal_read_line<S: std::io::Read>(stream: &mut S, pid: u32, echo: bool, name: &str) -> std::io::Result<String> {
    let mut ret = String::new();

    // Read one char at a time
    let mut buf: [u8; 1] = [0; 1]; 
    loop {
        stream.read_exact(&mut buf)?;
        ret.push(buf[0] as char);
        if buf[0] as char == '\n' {
            break
        }
    }
    
    if echo && !buf.is_empty() {
        print!("pid {} {}: {}", pid, name, ret);
    }

    return Ok(ret)
}

// Read what's left in the output pipe.
pub fn internal_drain_stream<S: std::io::Read>(stream: &mut S, pid: u32, echo: bool, name: &str) -> std::io::Result<()> {
    loop {
        match internal_read_line(stream, pid, echo, name) {
            Ok(buf) => {
                if buf.len() == 0 {
                    return Ok(());
                }
            },
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    // Hit EOF, exit.
                    return Ok(());
                }
                return Err(e);
            }
        }                
    }
}

// Write line to stream, blocking.
// Appends '/n' to end of 'line' string.
pub fn internal_write_line<S: std::io::Write>(stream: &mut S, line: &str, pid: u32, echo: bool, name: &str) -> std::io::Result<()> {
    let out = format!("{}\n", line);
    if echo {
        print!("pid {} {}: {}", pid, name, out);
    }
    stream.write_all(out.as_bytes())?;
    stream.flush()?;

    return Ok(())
}

#[macro_export]
macro_rules! expect_eq {
    ($left:expr, $right:expr) => {{
        if $left != $right {
            eprintln!(
                "Assertion failed at {}:{}: `{}` != `{}`, Left: `{:?}` Right: `{:?}`, expected equal.",
                file!(),
                line!(),
                stringify!($left),
                stringify!($right),
                $left,
                $right
            );
            false
        } else {
            true
        }
    }};
}

#[macro_export]
macro_rules! expect_ne {
    ($left:expr, $right:expr) => {{
        if $left == $right {
            eprintln!(
                "Assertion failed at {}:{}: `{}` == `{}`, Left: `{:?}` Right: `{:?}`, expected not equal.",
                file!(),
                line!(),
                stringify!($left),
                stringify!($right),
                $left,
                $right
            );
            false
        } else {
            true
        }
    }};
}

#[macro_export]
macro_rules! expect_gt {
    ($left:expr, $right:expr) => {{
        if $left <= $right {
            eprintln!(
                "Assertion failed at {}:{}: `{}` <= `{}`, Left: `{:?}` Right: `{:?}`, expected greater than.",
                file!(),
                line!(),
                stringify!($left),
                stringify!($right),
                $left,
                $right
            );
            false
        } else {
            true
        }
    }};
}

#[macro_export]
macro_rules! expect_lt {
    ($left:expr, $right:expr) => {{
        if $left >= $right {
            eprintln!(
                "Assertion failed at {}:{}: `{}` >= `{}`, Left: `{:?}` Right: `{:?}`, expected less than.",
                file!(),
                line!(),
                stringify!($left),
                stringify!($right),
                $left,
                $right
            );
            false
        } else {
            true
        }
    }};
}

#[macro_export]
macro_rules! expect_ge {
    ($left:expr, $right:expr) => {{
        if $left < $right {
            eprintln!(
                "Assertion failed at {}:{}: `{}` < `{}`, Left: `{:?}` Right: `{:?}`, expected greater or equal.",
                file!(),
                line!(),
                stringify!($left),
                stringify!($right),
                $left,
                $right
            );
            false
        } else {
            true
        }
    }};
}

#[macro_export]
macro_rules! expect_le {
    ($left:expr, $right:expr) => {{
        if $left > $right {
            eprintln!(
                "Assertion failed at {}:{}: `{}` > `{}`, Left: `{:?}` Right: `{:?}`, expected less or equal.",
                file!(),
                line!(),
                stringify!($left),
                stringify!($right),
                $left,
                $right
            );
            false
        } else {
            true
        }
    }};
}