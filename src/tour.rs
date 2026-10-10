//! Scripted camera flight through the repository, used for demos and
//! recordings (`--tour`, `T`).

use crate::camera::{v3, Camera, V3};
use crate::layout::{Layout, COL_CHARS, LINE_H};
use crate::scene::overview;

pub struct Shot {
    cam: Camera,
    travel: f32,
    hold: f32,
    /// Camera drift per second while holding.
    drift: V3,
    yaw_drift: f32,
    caption: Option<String>,
}

pub struct Tour {
    shots: Vec<Shot>,
    from: Camera,
    index: usize,
    t: f32,
    looped: bool,
}

/// Screen height the tour's reading distance is tuned for.
const REF_HEIGHT: f32 = 1080.0;

fn dir_view(l: &Layout, path: &str) -> Option<Camera> {
    let d = &l.dirs[l.find_dir(path)?];
    let (cx, cz) = d.rect.center();
    Some(Camera {
        target: v3(cx, d.y_top, cz + d.rect.h * 0.05),
        dist: d.rect.w.max(d.rect.h * 1.3) * 0.95,
        yaw: -0.18,
        pitch: 0.92,
    })
}

fn file_view(l: &Layout, path: &str, sources_paths: &[&str]) -> Option<(Camera, usize)> {
    let i = l.files.iter().position(|f| sources_paths[f.file] == path)?;
    let f = &l.files[i];
    let focal = REF_HEIGHT * 0.5 / (crate::camera::FOV_Y * 0.5).tan();
    let dist = LINE_H * focal / 15.0;
    let cam = Camera {
        target: v3(
            f.tile.x + COL_CHARS as f32 * 0.42,
            f.y,
            f.tile.z + dist * 0.28,
        ),
        dist,
        yaw: 0.0,
        pitch: 1.12,
    };
    Some((cam, i))
}

/// One stop on the tour: a directory overview, an optional low pass across
/// it, and an optional file read at reading distance. Each part is skipped
/// when its path is not on the map.
struct Stop {
    dir: Option<(String, String)>,
    low_pass: Option<String>,
    file: Option<(String, String)>,
}

/// A file only flop-core has; its presence selects flop-core's own tour.
const FLOP_CORE_SIGNATURE: &str = "pallets/pallets/has-station/src/lib.rs";

/// Top-level directories a derived tour visits, largest first.
const DERIVED_DIRS: usize = 4;

/// Of those, how many also get a file read.
const DERIVED_READS: usize = 2;

/// Extensions a derived tour avoids reading when the directory has source.
const NOT_SOURCE: &[&str] = &["md", "json", "yml", "yaml", "toml"];

/// flop-core's hand-written tour.
fn flop_core_stops() -> Vec<Stop> {
    let s = |x: &str| x.to_string();
    vec![
        Stop {
            dir: Some((
                s("pallets"),
                s("pallets/  Substrate FRAME runtime + consensus"),
            )),
            low_pass: Some(s("every block is a directory, every tile a file")),
            file: Some((
                s(FLOP_CORE_SIGNATURE),
                s("has-station: HTLC + policy escrow pallet"),
            )),
        },
        Stop {
            dir: Some((s("sdk"), s("sdk/  TypeScript, Python and agent SDKs"))),
            low_pass: None,
            file: Some((
                s("sdk/typescript/src/x402.ts"),
                s("x402.ts: pay-per-request inference"),
            )),
        },
        Stop {
            dir: Some((s("formal-specs"), s("formal-specs/  Quint + Lean models"))),
            low_pass: None,
            file: None,
        },
        Stop {
            dir: Some((s("explorer"), s("explorer/  Next.js block explorer"))),
            low_pass: None,
            file: None,
        },
    ]
}

/// A tour of the same shape for any other repository: its largest top-level
/// directories, a low pass over the first, and the largest source file of
/// the first few read up close. A repository with no directories reads its
/// largest file instead.
fn derived_stops(l: &Layout, paths: &[&str]) -> Vec<Stop> {
    let mut top = l.dirs[0].dirs.clone();
    top.sort_by(|a, b| {
        l.dirs[*b]
            .lines
            .cmp(&l.dirs[*a].lines)
            .then_with(|| l.dirs[*a].name.cmp(&l.dirs[*b].name))
    });
    let mut stops: Vec<Stop> = top
        .iter()
        .take(DERIVED_DIRS)
        .enumerate()
        .map(|(rank, &d)| {
            let dn = &l.dirs[d];
            let prefix = format!("{}/", dn.path);
            let inside: Vec<usize> = (0..l.files.len())
                .filter(|&f| paths[l.files[f].file].starts_with(&prefix))
                .collect();
            Stop {
                dir: Some((
                    dn.path.clone(),
                    format!(
                        "{}/  {} {}, {} lines",
                        dn.name,
                        inside.len(),
                        if inside.len() == 1 { "file" } else { "files" },
                        count(dn.lines)
                    ),
                )),
                low_pass: (rank == 0)
                    .then(|| "every block is a directory, every tile a file".to_string()),
                file: (rank < DERIVED_READS)
                    .then(|| read(l, paths, &inside))
                    .flatten(),
            }
        })
        .collect();
    if stops.is_empty() {
        let all: Vec<usize> = (0..l.files.len()).collect();
        if let Some(file) = read(l, paths, &all) {
            stops.push(Stop {
                dir: None,
                low_pass: None,
                file: Some(file),
            });
        }
    }
    stops
}

/// The file worth reading among `files` (layout indices): the longest source
/// file, or the longest of any kind when there is no source. Ties go to the
/// first path.
fn read(l: &Layout, paths: &[&str], files: &[usize]) -> Option<(String, String)> {
    let source = |f: &usize| {
        let ext = paths[l.files[*f].file].rsplit('.').next().unwrap_or("");
        !NOT_SOURCE.contains(&ext)
    };
    let longest = |pool: &mut dyn Iterator<Item = &usize>| {
        pool.min_by(|a, b| {
            l.files[**b]
                .lines
                .cmp(&l.files[**a].lines)
                .then_with(|| paths[l.files[**a].file].cmp(paths[l.files[**b].file]))
        })
        .copied()
    };
    let f =
        longest(&mut files.iter().filter(|f| source(f))).or_else(|| longest(&mut files.iter()))?;
    let node = &l.files[f];
    Some((
        paths[node.file].to_string(),
        format!("{}: {} lines", node.name, count(node.lines)),
    ))
}

/// A line count as a caption shows it: 940, 12.3k, 1.04M.
fn count(n: usize) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1e3)
    } else {
        format!("{:.2}M", n as f64 / 1e6)
    }
}

impl Tour {
    pub fn for_repo(l: &Layout, paths: &[&str], looped: bool) -> Tour {
        let home = overview(l);
        let mut shots = vec![Shot {
            cam: home,
            travel: 0.0,
            hold: 3.0,
            drift: v3(0.0, 0.0, 0.0),
            yaw_drift: 0.02,
            caption: Some(format!(
                "{}: {} files, {:.2}M lines",
                l.dirs[0].name,
                l.files.len(),
                l.total_lines as f64 / 1e6
            )),
        }];
        let dir = |shots: &mut Vec<Shot>, path: &str, caption: &str| {
            if let Some(cam) = dir_view(l, path) {
                shots.push(Shot {
                    cam,
                    travel: 3.2,
                    hold: 1.8,
                    drift: v3(0.0, 0.0, 0.0),
                    yaw_drift: 0.025,
                    caption: Some(caption.to_string()),
                });
            }
        };
        let file = |shots: &mut Vec<Shot>, path: &str, caption: &str| {
            if let Some((cam, f)) = file_view(l, path, paths) {
                let pan = (l.files[f].lines_per_col as f32 * LINE_H).min(160.0);
                shots.push(Shot {
                    cam,
                    travel: 3.6,
                    hold: 5.5,
                    drift: v3(0.0, 0.0, pan / 5.5),
                    yaw_drift: 0.0,
                    caption: Some(caption.to_string()),
                });
            }
        };
        let stops = if paths.contains(&FLOP_CORE_SIGNATURE) {
            flop_core_stops()
        } else {
            derived_stops(l, paths)
        };
        for stop in &stops {
            if let Some((path, caption)) = &stop.dir {
                dir(&mut shots, path, caption);
            }
            let low = stop.dir.as_ref().and_then(|(p, _)| l.find_dir(p));
            if let (Some(caption), Some(d)) = (&stop.low_pass, low) {
                // Low pass across the district so the blocks read as terrain.
                let r = l.dirs[d].rect;
                let hold = 5.0;
                shots.push(Shot {
                    cam: Camera {
                        target: v3(r.x + r.w * 0.25, l.dirs[d].y_top, r.z + r.h * 0.45),
                        dist: r.w * 0.32,
                        yaw: 0.55,
                        pitch: 0.42,
                    },
                    travel: 3.0,
                    hold,
                    drift: v3(r.w * 0.45 / hold, 0.0, r.h * 0.1 / hold),
                    yaw_drift: -0.06,
                    caption: Some(caption.clone()),
                });
            }
            if let Some((path, caption)) = &stop.file {
                file(&mut shots, path, caption);
            }
        }
        let mut low = home;
        low.pitch = 0.55;
        low.yaw = 0.5;
        low.dist *= 0.8;
        shots.push(Shot {
            cam: low,
            travel: 4.0,
            hold: 2.5,
            drift: v3(0.0, 0.0, 0.0),
            yaw_drift: 0.05,
            caption: None,
        });
        Tour {
            from: home,
            shots,
            index: 0,
            t: 0.0,
            looped,
        }
    }

    pub fn start_from(&mut self, cam: Camera) {
        self.from = cam;
        self.index = 0;
        self.t = 0.0;
        self.shots[0].travel = 2.5;
    }

    pub fn caption(&self) -> Option<&str> {
        let s = self.shots.get(self.index)?;
        // Show the caption once the camera is most of the way there.
        (self.t > s.travel * 0.55)
            .then_some(s.caption.as_deref())
            .flatten()
    }

    /// Advances by `dt` seconds; returns the camera, or None when finished.
    pub fn advance(&mut self, dt: f64) -> Option<Camera> {
        self.t += dt as f32;
        loop {
            let s = self.shots.get(self.index)?;
            if self.t < s.travel + s.hold {
                break;
            }
            self.t -= s.travel + s.hold;
            self.from = held(s, s.hold);
            self.index += 1;
            if self.index == self.shots.len() {
                if !self.looped {
                    return None;
                }
                self.index = 0;
                self.shots[0].travel = 4.0;
            }
        }
        let s = &self.shots[self.index];
        if self.t < s.travel {
            let u = self.t / s.travel;
            let e = u * u * (3.0 - 2.0 * u);
            let mut c = self.from.lerp(&s.cam, e);
            // Rise on long hops so the flight reads as a flight.
            let hop = self.from.target.sub(s.cam.target).len();
            let low = self.from.dist.min(s.cam.dist);
            let lift = (hop / low).max(1.0).ln() * 0.55;
            c.dist *= (lift * (std::f32::consts::PI * e).sin()).exp();
            Some(c)
        } else {
            Some(held(s, self.t - s.travel))
        }
    }
}

fn held(s: &Shot, t: f32) -> Camera {
    let mut c = s.cam;
    c.target = c.target.add(s.drift.scale(t));
    c.yaw += s.yaw_drift * t;
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::SourceFile;

    /// A source file of `lines` short lines; the tour only reads paths and sizes.
    fn file(path: &str, lines: usize) -> SourceFile {
        let text = vec![b'x'; lines * 2 - 1];
        SourceFile {
            path: path.to_string(),
            class: vec![0; text.len()],
            text,
            line_starts: (0..lines as u32).map(|i| i * 2).collect(),
        }
    }

    fn captions(files: &[(&str, usize)]) -> Vec<String> {
        let sources: Vec<SourceFile> = files.iter().map(|(p, n)| file(p, *n)).collect();
        let layout = Layout::build(&sources, "repo");
        let paths: Vec<&str> = sources.iter().map(|s| s.path.as_str()).collect();
        Tour::for_repo(&layout, &paths, false)
            .shots
            .iter()
            .skip(1) // the overview
            .filter_map(|s| s.caption.clone())
            .collect()
    }

    #[test]
    fn any_repo_tours_its_largest_directories() {
        // Five top-level directories: the four largest are visited in size
        // order, and the two largest also get a file read: the longest source
        // file (mod.rs over a longer README), or in docs/ the longest of any.
        let got = captions(&[
            ("README.md", 40),
            ("docs/guide.md", 900),
            ("src/app.rs", 300),
            ("src/store/mod.rs", 1200),
            ("src/store/README.md", 5000),
            ("tests/a.rs", 200),
            ("tests/b.rs", 250),
            ("edge/worker.js", 120),
            ("misc/x.sh", 10),
        ]);
        assert_eq!(
            got,
            [
                "src/  3 files, 6.5k lines",
                "every block is a directory, every tile a file",
                "mod.rs: 1.2k lines",
                "docs/  1 file, 900 lines",
                "guide.md: 900 lines",
                "tests/  2 files, 450 lines",
                "edge/  1 file, 120 lines",
            ]
        );
    }

    #[test]
    fn a_flat_repo_reads_its_longest_file() {
        let got = captions(&[("a.py", 30), ("b.py", 80), ("notes.md", 500)]);
        assert_eq!(got, ["b.py: 80 lines"]);
    }

    #[test]
    fn flop_core_keeps_its_own_tour() {
        // Only the signature file selects the hand-written stops; a repo that
        // merely has an `sdk/` directory gets a derived tour.
        let flop = captions(&[
            (FLOP_CORE_SIGNATURE, 400),
            ("pallets/other/src/lib.rs", 400),
            ("sdk/typescript/src/x402.ts", 200),
            ("formal-specs/channel/a.qnt", 100),
            ("explorer/app.ts", 100),
        ]);
        assert_eq!(
            flop,
            [
                "pallets/  Substrate FRAME runtime + consensus",
                "every block is a directory, every tile a file",
                "has-station: HTLC + policy escrow pallet",
                "sdk/  TypeScript, Python and agent SDKs",
                "x402.ts: pay-per-request inference",
                "formal-specs/  Quint + Lean models",
                "explorer/  Next.js block explorer",
            ]
        );
        let other = captions(&[("sdk/client.ts", 100)]);
        assert_eq!(other[0], "sdk/  1 file, 100 lines");
    }

    #[test]
    fn line_counts_read_at_a_glance() {
        assert_eq!(count(940), "940");
        assert_eq!(count(12_345), "12.3k");
        assert_eq!(count(1_036_000), "1.04M");
    }
}
