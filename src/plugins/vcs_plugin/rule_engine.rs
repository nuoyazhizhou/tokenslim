use super::ir::{VcsDocKind, VcsDocument, VcsRecord};
use super::types::VcsTool;

pub struct VcsRuleEngine;

impl VcsRuleEngine {
    pub fn normalize(mut doc: VcsDocument) -> VcsDocument {
        for record in &mut doc.records {
            match record {
                VcsRecord::Date(date) => {
                    *date = date.trim().to_string();
                }
                VcsRecord::Subject(subject) => {
                    *subject = subject.trim().to_string();
                }
                VcsRecord::Hunk(hunk) | VcsRecord::Stat(hunk) | VcsRecord::Raw(hunk) => {
                    *hunk = hunk.trim_end().to_string();
                }
                VcsRecord::Patch(_) => {}
                VcsRecord::DiffFile { left, right } => {
                    *left = left.trim().to_string();
                    *right = right.trim().to_string();
                }
                VcsRecord::LabeledFile { label, path } => {
                    *label = label.trim().to_string();
                    *path = path.trim().to_string();
                }
                VcsRecord::File { path, .. } => {
                    *path = path.trim().to_string();
                }
                _ => {}
            }
        }
        doc
    }

    pub fn render_text(doc: &VcsDocument) -> String {
        render_vcs_document(doc)
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_vcs_document(doc: &VcsDocument) -> String {
    let mut out = String::new();
    for rec in &doc.records {
        match rec {
            VcsRecord::Branch(branch) => {
                out.push_str("branch: ");
                out.push_str(branch);
                out.push('\n');
            }
            VcsRecord::Section(name) => {
                out.push_str(name);
                out.push_str(":\n");
            }
            VcsRecord::File {
                status: Some(status),
                path,
            } => {
                if *status == '?'
                    && matches!(doc.kind, VcsDocKind::Status)
                    && matches!(doc.tool, VcsTool::Git)
                {
                    out.push_str("?? ");
                } else {
                    out.push(*status);
                    out.push(' ');
                }
                out.push_str(path);
                out.push('\n');
            }
            VcsRecord::File { status: None, path } => {
                out.push_str(path);
                out.push('\n');
            }
            VcsRecord::LabeledFile { label, path } => {
                out.push_str(label);
                if label == "Removing" || label == "Would remove" {
                    out.push(' ');
                } else {
                    out.push_str(": ");
                }
                out.push_str(path);
                out.push('\n');
            }
            VcsRecord::Commit(hash) => {
                out.push_str("commit ");
                out.push_str(hash);
                out.push('\n');
            }
            VcsRecord::Author(author) => {
                out.push_str(author);
                out.push('\n');
            }
            VcsRecord::Date(date) => {
                out.push_str("Date: ");
                out.push_str(date);
                out.push('\n');
            }
            VcsRecord::Subject(subject) => {
                out.push_str(subject);
                out.push('\n');
            }
            VcsRecord::DiffFile { left, right } => {
                if matches!(doc.tool, VcsTool::Git) {
                    out.push_str("diff --git ");
                    out.push_str(left);
                    out.push(' ');
                    out.push_str(right);
                    out.push('\n');
                }
            }
            VcsRecord::Hunk(hunk)
            | VcsRecord::Patch(hunk)
            | VcsRecord::Stat(hunk)
            | VcsRecord::Raw(hunk) => {
                out.push_str(hunk);
                out.push('\n');
            }
        }
    }
    out
}
