//! Which packets go to the core, worked out from the tables and the settings.
//!
//! A pure function, so every case (one program, several, a program that is
//! not there, a PID that names a stream) is a test with no network in it.

use super::tables::{Program, Stream};

/// What the operator asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    /// A program number, or 0 for the first one the PAT lists.
    pub program: u16,
    /// Elementary stream PIDs to keep. Empty keeps every stream of the
    /// program. Naming PIDs without a program picks the program they are in.
    pub pids: Vec<u16>,
}

impl Choice {
    fn explicit(&self) -> bool {
        self.program != 0 || !self.pids.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// No PAT yet, or the chosen program's PMT has not arrived.
    Waiting,
    /// One program and nothing chosen: everything but stuffing goes through
    /// untouched, tables and all.
    PassAll,
    /// One program out of several, or some of its streams.
    Only(Selection),
    /// What was asked for is not in this feed. The words say what is.
    Missing(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub program: u16,
    pub pmt_pid: u16,
    /// Every PID that goes through: the PMT, the PCR and the chosen streams.
    pub keep: Vec<u16>,
    /// The streams to list in a rebuilt PMT, or `None` to pass the original.
    pub rewrite: Option<Vec<u16>>,
}

/// `programs` in PAT order; `parsed` says which have had their PMT read.
pub fn plan(choice: &Choice, programs: &[Program], parsed: &dyn Fn(u16) -> bool) -> Plan {
    if programs.is_empty() {
        return Plan::Waiting;
    }
    if !choice.explicit() && programs.len() == 1 {
        return Plan::PassAll;
    }
    let target = if choice.program != 0 {
        match programs.iter().find(|p| p.number == choice.program) {
            Some(p) => p,
            None => return Plan::Missing(not_there(&format!("program {}", choice.program), programs)),
        }
    } else if !choice.pids.is_empty() {
        match programs.iter().find(|p| p.streams.iter().any(|s| choice.pids.contains(&s.pid))) {
            Some(p) => p,
            None if programs.iter().any(|p| !parsed(p.pmt_pid)) => return Plan::Waiting,
            None => return Plan::Missing(not_there(&pid_words(&choice.pids), programs)),
        }
    } else {
        &programs[0]
    };
    if !parsed(target.pmt_pid) {
        return Plan::Waiting;
    }
    select(choice, target, programs)
}

fn select(choice: &Choice, p: &Program, programs: &[Program]) -> Plan {
    let chosen: Vec<&Stream> = if choice.pids.is_empty() {
        p.streams.iter().collect()
    } else {
        p.streams.iter().filter(|s| choice.pids.contains(&s.pid)).collect()
    };
    if chosen.is_empty() {
        let what = format!("{} in program {}", pid_words(&choice.pids), p.number);
        return Plan::Missing(not_there(&what, programs));
    }
    let mut keep = vec![p.pmt_pid, p.pcr_pid];
    keep.extend(chosen.iter().map(|s| s.pid));
    keep.sort_unstable();
    keep.dedup();
    let rewrite = (chosen.len() < p.streams.len()).then(|| chosen.iter().map(|s| s.pid).collect());
    Plan::Only(Selection { program: p.number, pmt_pid: p.pmt_pid, keep, rewrite })
}

fn pid_words(pids: &[u16]) -> String {
    let list: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
    format!("PID {}", list.join(", "))
}

/// "program 9 is not in this feed. It carries 1 (Sport): h264 PID 256, ...".
fn not_there(what: &str, programs: &[Program]) -> String {
    let all: Vec<String> = programs
        .iter()
        .map(|p| {
            let streams: Vec<String> =
                p.streams.iter().map(|s| format!("{} PID {}", s.kind(), s.pid)).collect();
            format!("{} [{}]", p.label(), streams.join(", "))
        })
        .collect();
    format!(
        "{what} is not in this feed. It carries {}. Choose one of those in the source's \
         settings, or set program to 0 for the first.",
        all.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(number: u16, pids: &[(u16, u8)]) -> Program {
        Program {
            number,
            pmt_pid: 0x1000 + number,
            pcr_pid: pids[0].0,
            streams: pids.iter().map(|&(pid, t)| Stream { pid, stream_type: t, info: vec![] }).collect(),
            ..Default::default()
        }
    }

    fn mux() -> Vec<Program> {
        vec![program(1, &[(256, 0x1B), (257, 0x0F)]), program(2, &[(512, 0x1B), (513, 0x0F), (514, 0x0F)])]
    }

    const ALL: &dyn Fn(u16) -> bool = &|_| true;

    #[test]
    fn one_program_and_no_choice_passes_everything() {
        let one = vec![program(1, &[(256, 0x1B)])];
        assert_eq!(plan(&Choice::default(), &one, ALL), Plan::PassAll);
    }

    #[test]
    fn several_programs_and_no_choice_take_the_first_the_pat_lists() {
        let Plan::Only(s) = plan(&Choice::default(), &mux(), ALL) else { panic!() };
        assert_eq!((s.program, s.keep, s.rewrite), (1, vec![256, 257, 0x1001], None));
    }

    #[test]
    fn a_pid_picks_its_program_and_the_pmt_is_rewritten_to_match() {
        let choice = Choice { program: 0, pids: vec![512, 514] };
        let Plan::Only(s) = plan(&choice, &mux(), ALL) else { panic!() };
        assert_eq!(s.program, 2);
        assert_eq!(s.rewrite, Some(vec![512, 514]));
    }

    #[test]
    fn a_program_that_is_not_there_says_what_is() {
        let Plan::Missing(why) = plan(&Choice { program: 9, pids: vec![] }, &mux(), ALL) else {
            panic!()
        };
        assert!(why.contains("program 9 is not in this feed"), "{why}");
        assert!(why.contains("2 [h264 PID 512, aac PID 513"), "{why}");
    }

    #[test]
    fn a_program_whose_pmt_has_not_arrived_waits() {
        let choice = Choice { program: 2, pids: vec![] };
        assert_eq!(plan(&choice, &mux(), &|pid| pid != 0x1002), Plan::Waiting);
    }
}
