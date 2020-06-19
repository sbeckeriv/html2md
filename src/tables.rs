use super::Config;
use super::StructuredPrinter;
use super::TagHandler;
use super::{clean_markdown, walk};

use std::{cmp, collections::HashMap};

use markup5ever_rcdom::{Handle, NodeData};

#[derive(Default)]
pub(super) struct TableHandler;

impl TagHandler for TableHandler {
    fn handle(&mut self, tag: &Handle, printer: &mut StructuredPrinter, config: &Config) {
        let mut table_markup = String::new();

        let td_matcher = |cell: &Handle| tag_name(cell) == "td";
        let th_matcher = |cell: &Handle| tag_name(cell) == "th";
        let any_matcher = |cell: &Handle| {
            let name = tag_name(cell);
            name == "td" || name == "th"
        };

        // detect cell width, counts
        let column_count: usize;
        let mut rows = find_children(tag, "tr");
        {
            // detect row count
            let most_big_row = rows.iter().max_by(|left, right| {
                collect_children(&left, any_matcher)
                    .len()
                    .cmp(&collect_children(&right, any_matcher).len())
            });
            if most_big_row.is_none() {
                // we don't have rows with content at all
                return;
            }
            // have rows with content, set column count
            column_count = collect_children(&most_big_row.unwrap(), any_matcher).len();
        }
        // max - 2 for the first line and space then 2 for the end of each column
        let column_width: usize = (config.max_length - column_count * 2 - 2) / column_count;

        table_markup.push('|');
        table_markup.push_str(&"-".repeat(config.max_length - 1));

        table_markup.push('|');
        table_markup.push('\n');
        {
            // add header row
            let header_tr = rows
                .iter()
                .find(|row| collect_children(&row, th_matcher).len() > 0);
            let header_cells = if let Some(header_row) = header_tr {
                collect_children(header_row, th_matcher)
            } else {
                Vec::new()
            };

            if header_cells.len() > 0 {
                let mut max_column_height = 0;
                let mut formated_columns = vec![];
                for index in 0..column_count {
                    let texts = if let Some(cell) = &header_cells.get(index) {
                        to_text(cell, config, column_width)
                    } else {
                        // no text in this cell, fill cell with spaces
                        vec![]
                    };
                    if texts.len() > max_column_height {
                        max_column_height = texts.len();
                    }
                    formated_columns.push(texts);
                }
                for height_index in 0..max_column_height {
                    table_markup.push('|');

                    for index in 0..column_count {
                        let default = "".to_string();
                        let text = if let Some(col) = formated_columns.get(index) {
                            if let Some(col_index) = col.get(height_index) {
                                col_index.clone()
                            } else {
                                default
                            }
                        } else {
                            default
                        };

                        table_markup.push_str(&format!(" {} ", pad_text(text, column_width)));
                        table_markup.push('|');
                    }
                    table_markup.push('\n');
                }

                // add header-body divider row
                table_markup.push('|');
                table_markup.push_str(&"-".repeat(config.max_length - 1));
                table_markup.push('|');
                table_markup.push('\n');
            }
        }

        // remove headers, leave only non-header rows now
        // process table rows
        rows.retain(|row| {
            let children = row.children.borrow();
            return children.iter().any(|child| tag_name(&child) == "td");
        });
        for row in &rows {
            let cells = collect_children(row, td_matcher);

            let mut formated_columns = vec![];
            let mut max_column_height = 0;

            for index in 0..column_count {
                let texts = if let Some(cell) = &cells.get(index) {
                    to_text(cell, config, column_width)
                } else {
                    // no text in this cell, fill cell with spaces
                    vec![]
                };
                if texts.len() > max_column_height {
                    max_column_height = texts.len();
                }
                formated_columns.push(texts);
            }

            for height_index in 0..max_column_height {
                table_markup.push('|');
                for index in 0..column_count {
                    let default = "".to_string();
                    let text = if let Some(col) = formated_columns.get(index) {
                        if let Some(col_index) = col.get(height_index) {
                            col_index.clone()
                        } else {
                            default
                        }
                    } else {
                        default
                    };

                    table_markup.push_str(&format!(" {} ", pad_text(text, column_width)));
                    table_markup.push('|');
                }
                table_markup.push('\n');
            }

            table_markup.push('|');
            table_markup.push_str(&"-".repeat(config.max_length - 1));
            table_markup.push('|');
            table_markup.push('\n');
        }

        printer.insert_newline();
        printer.insert_newline();
        printer.append_str(&table_markup);
    }

    fn after_handle(&mut self, _printer: &mut StructuredPrinter) {}

    fn skip_descendants(&self) -> bool {
        return true;
    }
}

fn column_height(tag: &Option<&Handle>, column_width: usize, config: &Config) -> usize {
    if let Some(cell) = tag {
        // have header at specified position
        to_text(cell, config, column_width).len()
    } else {
        // no text in this cell, fill cell with spaces
        1
    }
}

fn pad_text(text: String, column_width: usize) -> String {
    let mut result = "".to_string();
    // compute difference between width and text length
    let len_diff = column_width - text.chars().count();
    if len_diff > 0 {
        // should pad
        if len_diff > 1 {
            // should pad from both sides
            let pad_len = len_diff / 2;
            let remainder = len_diff % 2;
            result.push_str(&" ".repeat(pad_len));
            result.push_str(&text);
            result.push_str(&" ".repeat(pad_len + remainder));
        } else {
            // it's just one space, add at the end
            result.push_str(&text);
            result.push(' ');
        }
    } else {
        // shouldn't pad, text fills whole cell
        result.push_str(&text);
    }

    return result;
}

/// Extracts tag name from passed tag
/// Returns empty string if it's not an html element
fn tag_name(tag: &Handle) -> String {
    return match tag.data {
        NodeData::Element { ref name, .. } => name.local.to_string(),
        _ => String::new(),
    };
}

/// Find descendants of this tag with tag name `name`
/// This includes both direct children and descendants
fn find_children(tag: &Handle, name: &str) -> Vec<Handle> {
    let mut result: Vec<Handle> = vec![];
    let children = tag.children.borrow();
    for child in children.iter() {
        if tag_name(&child) == name {
            result.push(child.clone());
        }

        let mut descendants = find_children(&child, name);
        result.append(&mut descendants);
    }

    return result;
}

/// Collect direct children that satisfy the predicate
/// This doesn't include descendants
fn collect_children<P>(tag: &Handle, predicate: P) -> Vec<Handle>
where
    P: Fn(&Handle) -> bool,
{
    let mut result: Vec<Handle> = vec![];
    let children = tag.children.borrow();
    for child in children.iter() {
        let candidate = child.clone();
        if predicate(&candidate) {
            result.push(candidate);
        }
    }

    return result;
}

/// Convert html tag to text. This collects all tag children in correct order where they're observed
/// and concatenates their text, recursively.
fn to_text(tag: &Handle, config: &Config, column_width: usize) -> Vec<String> {
    let mut printer = StructuredPrinter::default();
    walk(tag, &mut printer, &HashMap::default(), config);
    let clean = clean_markdown(&printer.data);
    let clean = clean.replace("\n", "   ");
    config.break_size_string(&clean, column_width)
}
