use crate::{
    error::{new_error, ErrorKind, Result},
    grid::{Extent, Grid},
    GridValue, Pt, Ring,
};
use geo_types::Coord;
use lazy_static::lazy_static;
use rustc_hash::FxHashMap;
use slab::Slab;

lazy_static! {
    #[rustfmt::skip]
    static ref CASES: Vec<Vec<Vec<Vec<f64>>>> = vec![
        vec![],
        vec![vec![vec![1.0, 1.5], vec![0.5, 1.0]]],
        vec![vec![vec![1.5, 1.0], vec![1.0, 1.5]]],
        vec![vec![vec![1.5, 1.0], vec![0.5, 1.0]]],
        vec![vec![vec![1.0, 0.5], vec![1.5, 1.0]]],
        vec![
            vec![vec![1.0, 1.5], vec![0.5, 1.0]],
            vec![vec![1.0, 0.5], vec![1.5, 1.0]]
        ],
        vec![vec![vec![1.0, 0.5], vec![1.0, 1.5]]],
        vec![vec![vec![1.0, 0.5], vec![0.5, 1.0]]],
        vec![vec![vec![0.5, 1.0], vec![1.0, 0.5]]],
        vec![vec![vec![1.0, 1.5], vec![1.0, 0.5]]],
        vec![
            vec![vec![0.5, 1.0], vec![1.0, 0.5]],
            vec![vec![1.5, 1.0], vec![1.0, 1.5]]
        ],
        vec![vec![vec![1.5, 1.0], vec![1.0, 0.5]]],
        vec![vec![vec![0.5, 1.0], vec![1.5, 1.0]]],
        vec![vec![vec![1.0, 1.5], vec![1.5, 1.0]]],
        vec![vec![vec![0.5, 1.0], vec![1.0, 1.5]]],
        vec![]
    ];
}

#[derive(Clone, Debug)]
struct Fragment {
    start: usize,
    end: usize,
    ring: Ring,
}

/// Computes isoring for the given `Slice` of `values` according to the `threshold` value
/// (the inside of the isoring is the surface where input `values` are greater than or equal
/// to the given threshold value).
///
/// # Arguments
///
/// * `values` - The slice of values to be used.
/// * `threshold` - The threshold value.
/// * `dx` - The number of columns in the grid.
/// * `dy` - The number of rows in the grid.

pub fn contour_rings<V: GridValue, G: Grid<V>>(values: &G, threshold: V) -> Result<Vec<Ring>> {
    let mut isoring = IsoRingBuilder::new();
    isoring.compute(values, threshold)
}

/// Isoring generator to compute marching squares with isolines stitched into rings.
pub struct IsoRingBuilder {
    fragment_by_start: FxHashMap<usize, usize>,
    fragment_by_end: FxHashMap<usize, usize>,
    f: Slab<Fragment>,
    is_empty: bool,
}

impl IsoRingBuilder {
    /// Constructs a new IsoRing generator for a grid with `dx` * `dy` dimension.
    /// # Arguments
    ///
    /// * `dx` - The number of columns in the grid.
    /// * `dy` - The number of rows in the grid.
    pub fn new() -> Self {
        IsoRingBuilder {
            fragment_by_start: FxHashMap::default(),
            fragment_by_end: FxHashMap::default(),
            f: Slab::new(),
            is_empty: true,
        }
    }

    /// Computes isoring for the given slice of `values` according to the `threshold` value
    /// (the inside of the isoring is the surface where input `values` are greater than or equal
    /// to the given threshold value).
    ///
    /// # Arguments
    ///
    /// * `values` - The slice of values to be used.
    pub fn compute<V: GridValue, G: Grid<V>>(
        &mut self,
        values: &G,
        threshold: V,
    ) -> Result<Vec<Ring>> {
        let (width, _) = values.size();

        if !self.is_empty {
            self.clear();
        }
        let mut result = Vec::new();

        for Extent {
            top_left,
            bottom_right,
        } in values.extents()
        {
            for y in top_left.y..bottom_right.y {
                // t3 t2
                // t0 t1
                let mut t3 = values
                    .get_point(Coord::from((top_left.x, y)))
                    .map(|v| (v >= threshold) as usize);
                let mut t0 = values
                    .get_point(Coord::from((top_left.x, y + 1)))
                    .map(|v| (v >= threshold) as usize);
                let mut t2;
                let mut t1;
                for x in top_left.x + 1..=bottom_right.x {
                    t2 = values.get_point(Coord::from((x, y)));
                    t1 = values.get_point(Coord::from((x, y + 1)));
                    // TODO: Implement proper NODATA line extension as seen in GDAL (https://gdal.org/api/gdal_alg.html#_CPPv414GDAL_CG_Createiiiddd17GDALContourWriterPv)
                    if let (Some(v0), Some(v1), Some(v2), Some(v3)) = (t0, t1, t2, t3) {
                        let (v1, v2) = ((v1 >= threshold) as usize, (v2 >= threshold) as usize);
                        let idx = v0 | v1 << 1 | v2 << 2 | v3 << 3;
                        CASES[idx]
                            .iter()
                            .map(|ring| self.stitch(width, &ring, x - 1, y, &mut result))
                            .collect::<Result<Vec<()>>>()?;
                        t0 = Some(v1);
                        t3 = Some(v2);
                    } else {
                        t0 = t1.map(|v| (v >= threshold) as usize);
                        t3 = t2.map(|v| (v >= threshold) as usize);
                    }
                }
            }
        }
        self.is_empty = false;
        Ok(result)
    }

    fn index(&self, width: usize, point: &Pt) -> usize {
        (point.x * 2.0 + point.y * ((width + 2) * 4) as f64) as usize
    }

    // Stitches segments to rings.
    fn stitch(
        &mut self,
        width: usize,
        line: &[Vec<f64>],
        x: i64,
        y: i64,
        result: &mut Vec<Ring>,
    ) -> Result<()> {
        let start = Coord {
            x: line[0][0] + x as f64,
            y: line[0][1] + y as f64,
        };
        let end = Coord {
            x: line[1][0] + x as f64,
            y: line[1][1] + y as f64,
        };
        let start_index = self.index(width, &start);
        let end_index = self.index(width, &end);
        if self.fragment_by_end.contains_key(&start_index) {
            if self.fragment_by_start.contains_key(&end_index) {
                let f_ix = self
                    .fragment_by_end
                    .remove(&start_index)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                let g_ix = self
                    .fragment_by_start
                    .remove(&end_index)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                if f_ix == g_ix {
                    let mut f = self.f.remove(f_ix);
                    f.ring.push(end);
                    result.push(f.ring);
                } else {
                    let mut f = self.f.remove(f_ix);
                    let g = self.f.remove(g_ix);
                    f.ring.extend(g.ring);
                    let ix = self.f.insert(Fragment {
                        start: f.start,
                        end: g.end,
                        ring: f.ring,
                    });
                    self.fragment_by_start.insert(f.start, ix);
                    self.fragment_by_end.insert(g.end, ix);
                }
            } else {
                let f_ix = self
                    .fragment_by_end
                    .remove(&start_index)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                let f = self
                    .f
                    .get_mut(f_ix)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                f.ring.push(end);
                f.end = end_index;
                self.fragment_by_end.insert(end_index, f_ix);
            }
        } else if self.fragment_by_start.contains_key(&end_index) {
            if self.fragment_by_end.contains_key(&start_index) {
                let f_ix = self
                    .fragment_by_start
                    .remove(&end_index)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                let g_ix = self
                    .fragment_by_end
                    .remove(&start_index)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                if f_ix == g_ix {
                    let mut f = self.f.remove(f_ix);
                    f.ring.push(end);
                    result.push(f.ring);
                } else {
                    let f = self.f.remove(f_ix);
                    let mut g = self.f.remove(g_ix);
                    g.ring.extend(f.ring);
                    let ix = self.f.insert(Fragment {
                        start: g.start,
                        end: f.end,
                        ring: g.ring,
                    });
                    self.fragment_by_start.insert(g.start, ix);
                    self.fragment_by_end.insert(f.end, ix);
                }
            } else {
                let f_ix = self
                    .fragment_by_start
                    .remove(&end_index)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                let f = self
                    .f
                    .get_mut(f_ix)
                    .ok_or_else(|| new_error(ErrorKind::Unexpected))?;
                f.ring.insert(0, start);
                f.start = start_index;
                self.fragment_by_start.insert(start_index, f_ix);
            }
        } else {
            let ix = self.f.insert(Fragment {
                start: start_index,
                end: end_index,
                ring: vec![start, end],
            });
            self.fragment_by_start.insert(start_index, ix);
            self.fragment_by_end.insert(end_index, ix);
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.f.clear();
        self.fragment_by_end.clear();
        self.fragment_by_start.clear();
        self.is_empty = true;
    }
}
