//! The rename fix-it of K0405 (spec §10.1: "K0405, with a rename fix-it").
//!
//! A declaration that reuses a visible name gets a fix that renames it, at its declaring
//! occurrences (an or-pattern repeats one) and at every use that resolves to it. The new
//! name follows the skin's convention first (§10.1: in Arabic a value takes the article,
//! `الشكل: شكل`; in Franco `el_`, `el_donya: donya`; in English a value is lowercase), then
//! `name_2`, `name_3`, …; it is never spelled, or skeleton-folded (SEC0104), like any name
//! of the file, a prelude or core name, or a keyword, so applying the fix cannot cause a new
//! K0405, K0413, K0201 or SEC0104.
//!
//! The fix is machine-applicable when every occurrence is in the file and nothing outside
//! the file can name the declaration. A function's parameter is also its label at call
//! sites (§12.1), and an exported item may be imported by a module this check does not see,
//! so renaming either is only suggested. Imports, methods, fields and variants get no fix.

use std::collections::HashSet;

use kalam_base::{FileId, Skin, Span};
use kalam_diagnostics::{Applicability, Edit, Fix};

use super::Builder;
use crate::skeleton::skeleton;
use crate::{DefId, DefKind, NameRef, Res};

impl Builder<'_> {
    /// Attaches the rename fix to each K0405 that `bind` recorded.
    pub(super) fn rename_fixes(&mut self) {
        for (at, def) in std::mem::take(&mut self.renames) {
            if let Some(fix) = self.rename_fix(def) {
                if let Some(d) = self.diags.get_mut(at) {
                    d.fixes.push(fix);
                }
            }
        }
    }

    fn rename_fix(&self, def: DefId) -> Option<Fix> {
        let d = &self.p.defs[def.0 as usize];
        let machine = match d.kind {
            DefKind::Local | DefKind::LocalFn | DefKind::GenericParam => true,
            // A lambda's parameter has no label: only a function's does (§12.1).
            DefKind::Param => self.lambda_params.contains(&def),
            DefKind::Fn
            | DefKind::Record
            | DefKind::Enum
            | DefKind::Alias
            | DefKind::Trait
            | DefKind::Const => !d.exported,
            _ => return None,
        };
        let file = d.span.file;
        let mut spans: Vec<Span> = Vec::new();
        let mut occurrence =
            |r: &NameRef| (r.file == file).then(|| spans.push(Span::new(r.start, r.end)));
        for (r, target) in &self.p.decls {
            if *target == def {
                occurrence(r)?;
            }
        }
        for (r, res) in &self.p.uses {
            if *res == Res::Def(def) {
                occurrence(r)?;
            }
        }
        spans.sort_by_key(|s| (s.start, s.end));
        spans.dedup();
        if spans.is_empty() {
            return None;
        }
        let skin = self.sources.iter().find(|s| s.file == file)?.skin;
        // The skins' naming conventions are for values (§10.1, §13.3); a type, trait or
        // type parameter just gets a number.
        let value = matches!(
            d.kind,
            DefKind::Local | DefKind::Param | DefKind::LocalFn | DefKind::Fn | DefKind::Const
        );
        let new = self.fresh_name(&d.name, file, skin, value);
        Some(Fix {
            title: format!("rename this `{}` to `{new}`", d.name),
            applicability: if machine {
                Applicability::Machine
            } else {
                Applicability::Suggestion
            },
            edits: spans
                .into_iter()
                .map(|span| Edit {
                    span,
                    insert: new.clone(),
                })
                .collect(),
        })
    }

    /// A name for the renamed declaration that no name of `file`, of the prelude or of the
    /// core vocabulary is spelled or skeleton-folded like, and that is no keyword. A value
    /// (`value`) first tries its skin's convention.
    fn fresh_name(&self, name: &str, file: FileId, skin: Skin, value: bool) -> String {
        let mut names: HashSet<String> = self
            .p
            .defs
            .iter()
            .filter(|d| d.span.file == file)
            .map(|d| d.name.clone())
            .collect();
        names.extend(self.prelude_scope.names.keys().cloned());
        for skin_names in &self.core_names {
            names.extend(skin_names.iter().map(|(_, spelling, _)| spelling.clone()));
        }
        let skeletons: HashSet<String> = names.iter().map(|n| skeleton(n)).collect();
        let free = |n: &str| {
            !names.contains(n)
                && !skeletons.contains(&skeleton(n))
                && Skin::ALL
                    .iter()
                    .all(|s| self.table.skin(*s).lookup_word(n).is_none())
        };
        let first = name.chars().next().unwrap_or('_');
        let conventional = match skin {
            _ if !value => None,
            Skin::Ar if is_arabic_letter(first) && !name.starts_with("ال") => {
                Some(format!("ال{name}"))
            }
            Skin::Franco if first.is_ascii_lowercase() && !name.starts_with("el_") => {
                Some(format!("el_{name}"))
            }
            Skin::En if first.is_ascii_uppercase() => {
                Some(first.to_ascii_lowercase().to_string() + &name[first.len_utf8()..])
            }
            _ => None,
        };
        if let Some(c) = conventional.filter(|c| free(c)) {
            return c;
        }
        (2..)
            .map(|i| format!("{name}_{i}"))
            .find(|c| free(c))
            .unwrap_or_else(|| format!("{name}_2"))
    }
}

/// A letter of the Arabic block (the article goes only before an Arabic word: before a Latin
/// one it would make a mixed-script name, SEC0105).
fn is_arabic_letter(c: char) -> bool {
    ('\u{0620}'..='\u{064A}').contains(&c) || ('\u{066E}'..='\u{06D3}').contains(&c)
}
