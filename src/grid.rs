use crate::error::new_error;
use crate::{ErrorKind, GridValue, Result};
use geo_types::Coord;

pub trait Grid<V>
where
    V: GridValue,
{
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
            top_left: Coord::zero(),
            bottom_right: Coord::from((self.width as i64, self.height as i64)),
        })
    }

    fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn get_point(&self, coord: Coord<i64>) -> Option<V> {
        if coord.x < 0 || coord.y < 0 || coord.x > self.width as i64 || coord.y > self.height as i64
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
}

impl<const TILE_SIZE: usize, V: GridValue> Grid<V> for TiledBuffer<TILE_SIZE, V> {
    fn extents(&self) -> impl IntoIterator<Item = Extent> {
        self.tiles.iter().enumerate().filter_map(|(idx, v)| {
            if !v.is_empty() {
                let t_y = (idx / self.width) as i64;
                let t_x = (idx % self.width) as i64;
                let t_s = TILE_SIZE as i64;
                Some(Extent {
                    top_left: Coord::from((t_x * t_s, t_y * t_s)),
                    bottom_right: Coord::from(((t_x + 1) * t_s - 1, (t_y + 1) * t_s - 1)),
                })
            } else {
                None
            }
        })

        //Some(Extent { top_left: Coord::zero(), bottom_right: Coord::from(((self.width * TILE_SIZE) as i64 - 1, (self.height * TILE_SIZE) as i64- 1)) })
    }

    fn size(&self) -> (usize, usize) {
        (self.width * TILE_SIZE, self.height * TILE_SIZE)
    }

    fn get_point(&self, coord: Coord<i64>) -> Option<V> {
        if coord.x < 0 || coord.y < 0 {
            return None;
        }
        let (t_x, t_y) = (coord.x as usize / TILE_SIZE, coord.y as usize / TILE_SIZE);
        if t_x > self.width || t_y > self.height {
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
