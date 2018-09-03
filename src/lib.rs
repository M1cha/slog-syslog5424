//! A syslog 5424 drain for `slog` using a generic writer.
//!
//! # Important Info
//! Read the documentation on the underlying syslog5424 crate to see
//! the specifics on the formatting: []()
//!
//! Performance was not the main goal with this crate, so it may be
//! a bit slower than some other implementations:
//! * The buffer is not reused between messages
//! * When verifying/converting the message according to RFC5424, 3 String allocations
//! take place
//!
//! `slog-async` should probably almost always be used with this crate.

#![deny(unsafe_code, missing_docs)]

extern crate chrono;
extern crate slog;
extern crate syslog5424;

// re-exports
pub use syslog5424::iana::{Origin, TimeQuality};
pub use syslog5424::types::Facility;
pub use syslog5424::{Error, Rfc5424, Rfc5424Builder, WriteFormat};

use chrono::{SecondsFormat, Utc};
use slog::{Drain, Level, OwnedKVList, Record, Serializer, KV};
use syslog5424::types::{Message, Severity};
use syslog5424::{Rfc5424Data, StructuredData};

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Arguments;
use std::io::{self, Write};

/// Rfc5424 `slog` writer
#[derive(Debug)]
pub struct Rfc5424Writer<W: Write> {
    writer: RefCell<W>,
    formatter: Rfc5424,
}

impl<W: Write> Rfc5424Writer<W> {
    /// Create a new `Rfc5424Writer` which implements `slog::Drain`
    pub fn new(writer: W, formatter: Rfc5424) -> Rfc5424Writer<W> {
        Rfc5424Writer {
            writer: RefCell::new(writer),
            formatter,
        }
    }
}

/// Wrapper struct to store all the information provied by `slog`
/// for each log message. This way we can implement the trait
/// required for RFC5424 formatting on it
struct CompleteLogEntry<'a> {
    record: &'a Record<'a>,
    values: &'a OwnedKVList,
}

/// Wrapper for a vec so that we can implement `Serializer` on it.
struct StructuredWrapper(Vec<(String, String)>);

/// The most basic serializer. Convert `key` and `val` to strings
/// and store them as pairs in a vec.
impl<'a> Serializer for StructuredWrapper {
    fn emit_arguments(&mut self, key: slog::Key, val: &Arguments) -> slog::Result {
        self.0.push((key.to_string(), format!("{}", val)));
        Ok(())
    }
}

/// Allow `CompleteLogEntry` to be used as a data source for the
/// RFC5424 formatter
impl<'a> Rfc5424Data for CompleteLogEntry<'a> {
    fn severity(&self) -> Severity {
        match self.record.level() {
            Level::Critical => Severity::Critical,
            Level::Error => Severity::Error,
            Level::Warning => Severity::Warning,
            Level::Info => Severity::Informational,
            Level::Debug => Severity::Debug,
            Level::Trace => Severity::Debug, // TODO: is this right?
        }
    }

    fn timestamp(&self) -> Option<String> {
        Some(Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false))
    }

    fn structured_data(&self) -> Option<StructuredData> {
        let mut data: StructuredData = HashMap::new();
        let mut buf = StructuredWrapper(Vec::new());
        // our serializer never errors (only writes to a vec)
        self.record.kv().serialize(self.record, &mut buf).unwrap();
        self.values.serialize(self.record, &mut buf).unwrap();

        data.insert("slog", buf.0);
        Some(data)
    }

    fn message(&self) -> Option<Message> {
        Some(Message::Text(format!("{}", self.record.msg())))
    }
}

impl<W: Write> Drain for Rfc5424Writer<W> {
    type Ok = ();
    type Err = io::Error;

    fn log(&self, record: &Record, values: &OwnedKVList) -> Result<Self::Ok, Self::Err> {
        let msg = CompleteLogEntry { record, values };
        let mut writer = self.writer.borrow_mut();
        self.formatter.format(&mut *writer, &msg)
    }
}
