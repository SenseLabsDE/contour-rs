use crate::{error::new_error, ErrorKind, GridValue, Result};
use geo_types::Coord;

pub trait Grid<V>
where
    V: GridValue,
{
    /// Provides an iterator over relevant areas of the grid. Extents must not overlap and must extend one pixel beyond the line where contours should stop.
    /// In the case of a rectangular dataset, this means that the extent should add a single row/column on each side
    fn extents(&self) -> impl IntoIterator<Item = Extent>;
    fn size(&self) -> (usize, usize);
    fn get_point(&self, coord: Coord<i64>) -> Option<V>;
}

pub struct Extent {
    pub top_left: Coord<i64>,
    pub bottom_right: Coord<i64>,
}

pub struct Buffer<V: GridValue> {
    data: Vec<V>,
    width: usize,
    height: usize,
}

impl<V: GridValue> Buffer<V> {
    pub fn new(data: Vec<V>, width: usize, height: usize) -> Result<Self> {
        if data.len() != width * height {
            Err(new_error(ErrorKind::BadDimension))
        } else {
            Ok(Self {
                data,
                width,
                height,
            })
        }
    }

    pub fn data(&self) -> &[V] {
        &self.data
    }
}

impl<V: GridValue> Grid<V> for Buffer<V> {
    fn extents(&self) -> impl IntoIterator<Item = Extent> {
        Some(Extent {
            top_left: Coord::from((-1, -1)),
            bottom_right: Coord::from((self.width as i64, self.height as i64)),
        })
    }

    fn size(&self) -> (usize, usize) {
        (self.width + 2, self.height + 2)
    }

    fn get_point(&self, coord: Coord<i64>) -> Option<V> {
        if coord.x < 0
            || coord.y < 0
            || coord.x >= self.width as i64
            || coord.y >= self.height as i64
        {
            None
        } else {
            self.data
                .get(coord.y as usize * self.width + coord.x as usize)
                .copied()
        }
    }
}

pub struct TiledBuffer<const TILE_SIZE: usize, V: GridValue> {
    tiles: Vec<Vec<V>>,
    // width/height in tiles, not pixels!
    width: usize,
    height: usize,
}

impl<const TILE_SIZE: usize, V: GridValue> TiledBuffer<TILE_SIZE, V> {
    /// width and height in tiles, not pixels
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            tiles: vec![Vec::new(); width * height],
            width,
            height,
        }
    }

    pub fn set_tile(&mut self, x: usize, y: usize, data: Vec<V>) -> Result<()> {
        if data.len() != TILE_SIZE * TILE_SIZE || x > self.width || y > self.height {
            Err(new_error(ErrorKind::BadDimension))
        } else {
            self.tiles[y * self.width + x] = data;
            Ok(())
        }
    }

    fn has_tile(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            false
        } else {
            self.tiles
                .get(y as usize * self.width + x as usize)
                .filter(|v| !v.is_empty())
                .is_some()
        }
    }
}

impl<const TILE_SIZE: usize, V: GridValue> Grid<V> for TiledBuffer<TILE_SIZE, V> {
    // +-----------------------+
    // | 3 |      4        | 5 |
    // |---+---------------+---|
    // |   |               |   |
    // | 2 |      0        | 6 |
    // |   |               |   |
    // |---+---------------+---|
    // | 1 |      8        | 7 |
    // +-----------------------+
    // Each tile produces multiple extents to account for border regions
    // 0..=4 are always produced
    // 5..=8 are only produced if there is no neighbor in that direction (as it would include the same region in its 0..=4 extents)
    //
    // TODO: Investigate if merging extents meaningfully improves performance
    fn extents(&self) -> impl IntoIterator<Item = Extent> {
        self.tiles.iter().enumerate().flat_map(|(idx, v)| {
            if !v.is_empty() {
                let t_y = (idx / self.width) as i64;
                let t_x = (idx % self.width) as i64;
                let t_s = TILE_SIZE as i64;
                let top_left = Coord::from((t_x * t_s, t_y * t_s));
                let bottom_right = Coord::from(((t_x + 1) * t_s - 1, (t_y + 1) * t_s - 1));
                let mut extents = vec![
                    // 0
                    Extent {
                        top_left,
                        bottom_right,
                    },
                    // 1
                    Extent {
                        top_left: Coord::from((top_left.x - 1, bottom_right.y)),
                        bottom_right: Coord::from((top_left.x, bottom_right.y + 1)),
                    },
                    // 2
                    Extent {
                        top_left: Coord::from((top_left.x - 1, top_left.y)),
                        bottom_right: Coord::from((top_left.x, bottom_right.y)),
                    },
                    // 3
                    Extent {
                        top_left: Coord::from((top_left.x - 1, top_left.y - 1)),
                        bottom_right: top_left,
                    },
                    // 4
                    Extent {
                        top_left: Coord::from((top_left.x, top_left.y - 1)),
                        bottom_right: Coord::from((bottom_right.x, top_left.y)),
                    },
                ];
                // 5
                if self.has_tile(t_x + 1, t_y - 1) {
                    extents.push(Extent {
                        top_left: Coord::from((bottom_right.x, top_left.y - 1)),
                        bottom_right: Coord::from((bottom_right.x + 1, top_left.y)),
                    });
                }
                // 6
                if self.has_tile(t_x + 1, t_y) {
                    extents.push(Extent {
                        top_left: Coord::from((bottom_right.x, top_left.y)),
                        bottom_right: Coord::from((bottom_right.x + 1, bottom_right.y)),
                    });
                }
                // 7
                if self.has_tile(t_x + 1, t_y + 1) {
                    extents.push(Extent {
                        top_left: bottom_right,
                        bottom_right: Coord::from((bottom_right.x + 1, bottom_right.y + 1)),
                    });
                }
                // 8
                if self.has_tile(t_x, t_y + 1) {
                    extents.push(Extent {
                        top_left: Coord::from((top_left.x, bottom_right.y)),
                        bottom_right: Coord::from((bottom_right.x, bottom_right.y + 1)),
                    })
                }
                extents
            } else {
                Vec::new()
            }
        })
    }

    fn size(&self) -> (usize, usize) {
        (self.width * TILE_SIZE + 2, self.height * TILE_SIZE + 2)
    }

    fn get_point(&self, coord: Coord<i64>) -> Option<V> {
        if coord.x < 0 || coord.y < 0 {
            return None;
        }
        let (t_x, t_y) = (coord.x as usize / TILE_SIZE, coord.y as usize / TILE_SIZE);
        if t_x >= self.width || t_y >= self.height {
            None
        } else {
            let (rel_x, rel_y) = (coord.x as usize % TILE_SIZE, coord.y as usize % TILE_SIZE);
            self.tiles[t_y * self.width + t_x]
                .get(rel_y * TILE_SIZE + rel_x)
                .copied()
        }
    }
}

pub struct NoDataMask<V: GridValue, T: Grid<V>> {
    inner: T,
    no_data: V,
}

impl<V: GridValue, T: Grid<V>> NoDataMask<V, T> {
    pub fn new(inner: T, no_data: V) -> Self {
        Self { inner, no_data }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<V: GridValue, T: Grid<V>> Grid<V> for NoDataMask<V, T> {
    fn extents(&self) -> impl IntoIterator<Item = Extent> {
        self.inner.extents()
    }

    fn size(&self) -> (usize, usize) {
        self.inner.size()
    }

    fn get_point(&self, coord: Coord<i64>) -> Option<V> {
        self.inner.get_point(coord).filter(|&v| v != self.no_data)
    }
}
