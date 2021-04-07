use crate::{Point, PointIndex, EdgeIndex};
use crate::predicates::pseudo_angle;

const N: usize = 1 << 10;
const EMPTY: PointIndex = PointIndex(std::usize::MAX);

#[derive(Clone, Copy, Debug, Default)]
struct Node {
    // This is the point's absolute ordering.  It is assigned into a bucket
    // based on this order and the total bucket count
    angle: f64,

    edge: EdgeIndex,

    // prev and next refer to traveling counterclockwise around the hull
    prev: PointIndex,
    next: PointIndex,
}

/// The Hull stores a set of points which form a counterclockwise topological
/// circle about the center of the triangulation.
///
/// Each point is associated with an EdgeIndex into a half-edge data structure,
/// but the Hull does not concern itself with such things.
///
/// The Hull supports one kind of lookup: for a point P, find the point Q with
/// the highest psuedo-angle that is below P.  When projecting P towards the
/// triangulation center, it will intersect the edge beginning at Q; this
/// edge is the one which should be split.
#[derive(Debug)]
pub struct Hull {
    buckets: [PointIndex; N],
    data: Vec<Node>,
}
impl Default for Hull {
    fn default() -> Self {
        Hull {
            buckets: [PointIndex::default(); N],
            data: vec![],
        }
    }
}

impl Hull {
    pub fn new(center: Point, pts: &[Point]) -> Hull {
        // By default, nodes which aren't in the array have both edges linked
        // to EMPTY, so we can detect them when inserting.
        let mut data = Vec::with_capacity(pts.len());
        data.extend(pts.iter()
            .map(|p| Node {
                angle: pseudo_angle((p.0 - center.0, p.1 - center.1)),
                edge: EdgeIndex::default(),
                prev: EMPTY,
                next: EMPTY,
                }));

        Hull {
            buckets: [EMPTY; N],
            data,
        }
    }

    // Inserts the first point, along with its associated edge
    pub fn insert_first(&mut self, p: PointIndex, e: EdgeIndex) {
        let b = self.bucket(p);
        assert!(self.buckets[b] == EMPTY);
        self.buckets[b] = p;

        // Tie this point into a tiny loop
        self.data[p.0].next = p;
        self.data[p.0].prev = p;

        // Attach the edge index data to this point
        self.data[p.0].edge = e;
    }

    pub fn update(&mut self, p: PointIndex, e: EdgeIndex) {
        assert!(self.data[p.0].next != EMPTY);
        self.data[p.0].edge = e;
    }

    /// For a given point, returns a (prev, next) pair for the edge which
    /// that point intersects, when projected towards the triangulation center.
    pub fn get(&self, p: PointIndex) -> (PointIndex, PointIndex) {
        let b = self.bucket(p);

        // If the target bucket is empty, then we should search for the
        // next-highest point, then walk back one step to find the next-lowest
        // point.  This is better than searching for the next-lowest point,
        // which requires finding the next-lowest bucket then walking all
        // the way to the end of that bucket's chain.
        let mut pos = self.buckets[b];
        if pos == EMPTY {
            // Find the next filled bucket, which must exist somewhere
            let mut t = b;
            while self.buckets[t] == EMPTY {
                t = (t + 1) % N;
            }
            pos = self.buckets[t];
        } else {
            // This bucket is already occupied, so we'll need to walk its
            // linked list until we find the right place to insert.

            // Loop until we find an item in the linked list which is less
            // that our new point, or we leave this bucket, or we're about
            // to wrap around in the same bucket.
            let start = pos;
            while self.data[pos.0].angle < self.data[p.0].angle &&
                  self.bucket(pos) == b
            {
                pos  = self.data[pos.0].next;
                // If we've looped around, it means all points are in the same
                // bucket *and* the new point is larger than all of them.  This
                // means it will be insert at the end of the bucket, and will
                // link back to the first point in the bucket.
                if pos == start {
                    break;
                }
            }
        }

        // Walk backwards one step the list to find the previous node, then
        // return its edge data.
        let prev = self.data[pos.0].prev;
        (prev, pos)
    }

    pub fn get_edge(&self, p: PointIndex) -> EdgeIndex {
        let (prev, _) = self.get(p);
        self.data[prev.0].edge
    }

    pub fn prev_edge(&self, p: PointIndex) -> EdgeIndex {
        assert!(self.data[p.0].prev != EMPTY);
        self.edge(self.data[p.0].prev)
    }

    pub fn edge(&self, p: PointIndex) -> EdgeIndex {
        // Assert that this node is in the array
        assert!(self.data[p.0].next != EMPTY);
        self.data[p.0].edge
    }

    pub fn insert(&mut self, p: PointIndex, e: EdgeIndex) {
        // Assert that this node isn't in the array already
        assert!(self.data[p.0].next == EMPTY);
        let b = self.bucket(p);
        let (prev, next) = self.get(p);

        // If the target bucket is empty, or the given point is below the first
        // item in the target bucket, then it becomes the bucket's head
        if self.buckets[b] == EMPTY || (self.buckets[b] == next &&
            self.data[p.0].angle < self.data[next.0].angle)
        {
            self.buckets[b] = p;
        }

        // Write all of our new node data, leaving angle fixed
        self.data[p.0].edge = e;
        self.data[p.0].next = next;
        self.data[p.0].prev = prev;

        // Stitch ourselves into the linked list
        self.data[next.0].prev = p;
        self.data[prev.0].next = p;
    }

    /// Removes the given point from the hull
    pub fn erase(&mut self, p: PointIndex) {
        let b = self.bucket(p);

        let next = self.data[p.0].next;
        let prev = self.data[p.0].prev;

        // Cut this node out of the linked list
        self.data[next.0].prev = prev;
        self.data[prev.0].next = next;
        self.data[p.0].next = EMPTY;
        self.data[p.0].prev = EMPTY;

        // If this is the head of the bucket, then replace it with the next
        // item in this bucket chain (assuming it belongs in the same bucket),
        // or EMPTY if the bucket is now completely empty.
        if self.buckets[b] == p {
            if self.bucket(next) == b {
                self.buckets[b] = next;
            } else {
                self.buckets[b] = EMPTY;
            }
        }
    }

    /// Iterates over all edges stored in the Hull, in order
    pub fn values(&self) -> impl Iterator<Item=EdgeIndex> + '_ {
        // Find the first non-empty bucket to use as our starting point for
        // walking around the hull's linked list.
        let mut point: PointIndex = self.buckets.iter()
            .filter(|b| **b != EMPTY)
            .copied()
            .next()
            .unwrap();
        // Then, walk the linked list until we hit the starting point again,
        // returning the associated edges at each point.
        let start = point;
        let mut started = false;
        std::iter::from_fn(move || {
            if point == start && started {
                None
            } else {
                started = true;
                let out = self.data[point.0].edge;
                point = self.data[point.0].next;
                Some(out)
            }
        })
    }

    /// Looks up what bucket a given point will fall into
    fn bucket(&self, p: PointIndex) -> usize {
        (self.data[p.0].angle * (self.buckets.len() as f64 - 1.0)).round() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;
    use rand::seq::SliceRandom;

    #[test]
    fn circular_hull() {
        let mut pts = Vec::new();
        let num = 1_000_000;
        for i in 0..num {
            let angle = i as f64 * 2.0 * std::f64::consts::PI / (num as f64);
            pts.push((angle.cos(), angle.sin()));
        }
        pts.shuffle(&mut rand::thread_rng());

        let mut h = Hull::new((0.0, 0.0), &pts);
        h.insert_first(PointIndex(0), EdgeIndex(NonZeroUsize::new(1).unwrap()));
        for i in 1..num {
            if i % 1000 == 0 {
                eprintln!("{}", i);
            }
            h.insert(PointIndex(i), EdgeIndex(NonZeroUsize::new(2).unwrap()));
        }
    }
}
