// test:
// recursive dirs that link to each other.
// Pass in bad dir name
// num to search for is less than num available
// admin files.
//
extern crate ansi_term;
#[macro_use]
extern crate clap;

use std::collections::HashSet;

use ansi_term::Colour::Fixed;
use clap::{App, AppSettings, Arg};

use std::cmp;
use std::cmp::Ordering;
use std::fs;
use std::fs::ReadDir;
use std::io;

#[derive(Clone, Debug)]
struct Node {
    dir: Dir,
    children: Vec<Node>,
}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.dir.size > other.dir.size {
            Ordering::Less
        } else if self.dir.size < other.dir.size {
            Ordering::Greater
        } else {
            let my_slashes = self.dir.name.matches("/").count();
            let other_slashes = other.dir.name.matches("/").count();

            if my_slashes > other_slashes {
                Ordering::Greater
            } else if my_slashes < other_slashes {
                Ordering::Less
            } else {
                if self.dir.name < other.dir.name {
                    Ordering::Less
                } else if self.dir.name > other.dir.name {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            }
        }
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        (&self.dir.name, self.dir.size) == (&other.dir.name, other.dir.size)
    }
}
impl Eq for Node {}

#[derive(Clone, Debug)]
struct Dir {
    name: String,
    size: u64,
}

static DEFAULT_NUMBER_OF_LINES: &'static str = &"15";

fn main() {
    let options = App::new("Trailing args example")
        .setting(AppSettings::TrailingVarArg)
        .arg(
            Arg::with_name("number_of_lines")
                .short("n")
                .help("Number of lines of output to show")
                .takes_value(true)
                .default_value(DEFAULT_NUMBER_OF_LINES),
        )
        .arg(Arg::with_name("inputs").multiple(true))
        .get_matches();

    let filenames = {
        match options.values_of("inputs") {
            None => vec!["."],
            Some(r) => r.collect(),
        }
    };
    let number_of_lines = value_t!(options.value_of("number_of_lines"), usize).unwrap();

    let (permissions, results) = get_dir_tree(filenames);
    let slice_it = find_big_ones(&results, number_of_lines);
    display(permissions, slice_it);
}

fn get_dir_tree(filenames: Vec<&str>) -> (bool, Vec<Node>) {
    let mut permissions = true;
    let mut results = vec![];
    for b in filenames {
        let mut new_name = String::from(b);
        while new_name.chars().last() == Some('/') && new_name.len() != 1 {
            new_name.pop();
        }
        let (hp, data) = examine_dir_str(new_name);
        permissions = permissions && hp;
        results.push(data);
    }
    (permissions, results)
}

fn examine_dir_str(loc: String) -> (bool, Node) {
    let mut inodes: HashSet<u64> = HashSet::new();
    let (hp, result) = examine_dir(fs::read_dir(&loc), &mut inodes);

    // This needs to be folded into the below recursive call somehow
    let new_size = result.iter().fold(0, |a, b| a + b.dir.size);
    (
        hp,
        Node {
            dir: Dir {
                name: loc,
                size: new_size,
            },
            children: result,
        },
    )
}

#[cfg(target_os = "linux")]
fn get_metadata_blocks_and_inode(d: &std::fs::DirEntry) -> Option<(u64, u64)> {
    use std::os::linux::fs::MetadataExt;
    match d.metadata().ok() {
        Some(md) => Some((md.len(), md.st_ino())),
        None => None,
    }
}

#[cfg(target_os = "unix")]
fn get_metadata_blocks_and_inode(d: &std::fs::DirEntry) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    match d.metadata().ok() {
        Some(md) => Some((md.len(), md.ino())),
        None => None,
    }
}

#[cfg(target_os = "macos")]
fn get_metadata_blocks_and_inode(d: &std::fs::DirEntry) -> Option<(u64, u64)> {
    use std::os::macos::fs::MetadataExt;
    match d.metadata().ok() {
        Some(md) => Some((md.len(), md.st_ino())),
        None => None,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "unix", target_os = "macos")))]
fn get_metadata_blocks_and_inode(_d: &std::fs::DirEntry) -> Option<(u64, u64)> {
    match d.metadata().ok() {
        Some(md) => Some((md.len(), 0)), //move to option not 0
        None => None,
    }
}

fn examine_dir(a_dir: io::Result<ReadDir>, inodes: &mut HashSet<u64>) -> (bool, Vec<Node>) {
    let mut result = vec![];
    let mut have_permission = true;

    if a_dir.is_ok() {
        let paths = a_dir.unwrap();
        for dd in paths {
            match dd {
                Ok(d) => {
                    let file_type = d.file_type().ok();
                    let maybe_size_and_inode = get_metadata_blocks_and_inode(&d);

                    match (file_type, maybe_size_and_inode) {
                        (Some(file_type), Some((size, inode))) => {
                            let s = d.path().to_string_lossy().to_string();
                            if inodes.contains(&inode) {
                                continue;
                            }
                            inodes.insert(inode);

                            if d.path().is_dir() && !file_type.is_symlink() {
                                let (hp, recursive) = examine_dir(fs::read_dir(d.path()), inodes);
                                have_permission = have_permission && hp;
                                let new_size = recursive.iter().fold(size, |a, b| a + b.dir.size);
                                result.push(Node {
                                    dir: Dir {
                                        name: s,
                                        size: new_size,
                                    },
                                    children: recursive,
                                })
                            } else {
                                result.push(Node {
                                    dir: Dir {
                                        name: s,
                                        size: size,
                                    },
                                    children: vec![],
                                })
                            }
                        }
                        (_, None) => have_permission = false,
                        (_, _) => (),
                    }
                }
                Err(_) => (),
            }
        }
    } else {
        have_permission = false;
    }
    (have_permission, result)
}

// We start with a list of root directories - these must be the biggest folders
// We then repeadedly merge in the children of the biggest directory - Each iteration
// the next biggest directory's children are merged in.
fn find_big_ones<'a>(l: &'a Vec<Node>, max_to_show: usize) -> Vec<&Node> {
    let mut new_l: Vec<&Node> = l.iter().map(|a| a).collect();
    new_l.sort();

    for processed_pointer in 0..max_to_show {
        if new_l.len() == processed_pointer {
            break;
        }
        // Must be a list of pointers into new_l otherwise b_list will go out of scope
        // when it is deallocated
        let mut b_list: Vec<&Node> = new_l[processed_pointer]
            .children
            .iter()
            .map(|a| a)
            .collect();
        new_l.extend(b_list);
        new_l.sort();
        /*println!(
            "{:?} -------------------",
            new_l
                .iter()
                .map(|a| a.dir.size.to_string() + ": " + &a.dir.name)
                .collect::<Vec<String>>()
        );*/
    }
    if new_l.len() > max_to_show {
        new_l[0..max_to_show + 1].to_vec()
    } else {
        new_l
    }
}

fn display(permissions: bool, to_display: Vec<&Node>) -> () {
    if !permissions {
        eprintln!("Did not have permissions for all directories");
    }

    display_node(to_display[0], &to_display, true, 1, "")
}

fn display_node<S: Into<String>>(
    node_to_print: &Node,
    to_display: &Vec<&Node>,
    is_first: bool,
    depth: u8,
    indentation_str: S,
) {
    let mut is = indentation_str.into();
    print_this_node(node_to_print, is_first, depth, is.as_ref());

    is = is.replace("└──", "   ");
    is = is.replace("├──", "│  ");

    let printable_node_slashes = node_to_print.dir.name.matches("/").count();

    let mut num_sibblings = to_display.iter().fold(0, |a, b| {
        if node_to_print.children.contains(b)
            && b.dir.name.matches("/").count() == printable_node_slashes + 1
        {
            a + 1
        } else {
            a
        }
    });

    let mut is_biggest = true;
    for node in to_display {
        if node_to_print.children.contains(node) {
            if node.dir.name.matches("/").count() == printable_node_slashes + 1 {
                num_sibblings -= 1;
                let tree_chars = {
                    if num_sibblings == 0 {
                        "└──"
                    } else {
                        "├──"
                    }
                };
                display_node(
                    &node,
                    to_display,
                    is_biggest,
                    depth + 1,
                    is.to_string() + tree_chars,
                );
                is_biggest = false;
            }
        }
    }
}

fn print_this_node(node_to_print: &Node, is_biggest: bool, depth: u8, indentation_str: &str) {
    let padded_size = format!("{:>5}", human_readable_number(node_to_print.dir.size),);
    println!(
        "{} {} {}",
        if is_biggest {
            Fixed(196).paint(padded_size)
        } else {
            Fixed(7).paint(padded_size)
        },
        indentation_str,
        Fixed(7)
            .on(Fixed(cmp::min(8, (depth) as u8) + 231))
            .paint(node_to_print.dir.name.to_string())
    );
}

fn human_readable_number(size: u64) -> (String) {
    let units = vec!["T", "G", "M", "K"]; //make static

    //return format!("{}B", size);

    for (i, u) in units.iter().enumerate() {
        let marker = 1024u64.pow((units.len() - i) as u32);
        if size >= marker {
            if size / marker < 10 {
                return format!("{:.1}{}", (size as f32 / marker as f32), u);
            } else {
                return format!("{}{}", (size / marker), u);
            }
        }
    }
    return format!("{}B", size);
}

mod tests {
    use super::*;

    #[test]
    fn test_human_readable_number() {
        assert_eq!(human_readable_number(1), "1B");
        assert_eq!(human_readable_number(956), "956B");
        assert_eq!(human_readable_number(1004), "1004B");
        assert_eq!(human_readable_number(1024), "1.0K");
        assert_eq!(human_readable_number(1536), "1.5K");
        assert_eq!(human_readable_number(1024 * 512), "512K");
        assert_eq!(human_readable_number(1024 * 1024), "1.0M");
        assert_eq!(human_readable_number(1024 * 1024 * 1024 - 1), "1023M");
        assert_eq!(human_readable_number(1024 * 1024 * 1024 * 20), "20G");
        assert_eq!(human_readable_number(1024 * 1024 * 1024 * 1024), "1.0T");
    }
}