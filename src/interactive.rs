use std::io::{self, BufRead, Write};

use crate::entry::TimestampShape;

const TAGS_PROMPT: &str = "Tags (space-separated, optional): ";
const TIMESTAMP_PROMPT: &str =
    "Timestamp [F/full, y/year, d/month-day, t/time] (default F): ";
const ENTRY_PROMPT: &str = "Entry (press Ctrl-D on an empty line to submit):\n";

#[derive(Debug, PartialEq, Eq)]
pub struct InteractiveInput {
    pub tags: String,
    pub timestamp_shape: TimestampShape,
    pub body: String,
}

/// Prompts for an interactive entry. `Ok(None)` means stdin reached EOF
/// before the body prompt, so no entry should be written.
pub fn read_interactive<R: BufRead, W: Write>(
    input: &mut R,
    prompts: &mut W,
) -> io::Result<Option<InteractiveInput>> {
    write_prompt(prompts, TAGS_PROMPT)?;
    let Some(tags) = read_answer(input)? else {
        return Ok(None);
    };

    let timestamp_shape = loop {
        write_prompt(prompts, TIMESTAMP_PROMPT)?;
        let Some(answer) = read_answer(input)? else {
            return Ok(None);
        };
        match answer.trim().to_ascii_lowercase().as_str() {
            "" | "f" | "full" => break TimestampShape::Full,
            "y" | "year" => break TimestampShape::Year,
            "d" | "date" | "month-day" => break TimestampShape::MonthDay,
            "t" | "time" => break TimestampShape::Time,
            _ => {
                writeln!(
                    prompts,
                    "Invalid timestamp choice; use F/full, y/year, d/date/month-day, or t/time."
                )?;
                prompts.flush()?;
            }
        }
    };

    write_prompt(prompts, ENTRY_PROMPT)?;
    let mut body = String::new();
    input.read_to_string(&mut body)?;
    Ok(Some(InteractiveInput {
        tags,
        timestamp_shape,
        body,
    }))
}

fn write_prompt(output: &mut impl Write, prompt: &str) -> io::Result<()> {
    output.write_all(prompt.as_bytes())?;
    output.flush()
}

fn read_answer(input: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        return Ok(None);
    }
    Ok(Some(answer.trim_end_matches(['\r', '\n']).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn invalid_timestamp_reprompts() {
        let mut input = Cursor::new("tag\nnope\nyear\nbody\n");
        let mut output = Vec::new();
        let result = read_interactive(&mut input, &mut output).unwrap().unwrap();
        assert_eq!(result.timestamp_shape, TimestampShape::Year);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Invalid timestamp choice"));
        assert_eq!(output.matches(TIMESTAMP_PROMPT).count(), 2);
    }

    #[test]
    fn eof_at_an_early_prompt_aborts() {
        let mut output = Vec::new();
        assert_eq!(
            read_interactive(&mut Cursor::new(""), &mut output).unwrap(),
            None
        );
        output.clear();
        assert_eq!(
            read_interactive(&mut Cursor::new("tags\n"), &mut output).unwrap(),
            None
        );
    }
}
