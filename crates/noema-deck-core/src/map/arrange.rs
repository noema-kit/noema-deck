//! Arranging the whole map as one galaxy, for a view of every domain at once.
//!
//! The root domains gather at the core. Each level of subdomains lies on a
//! wider ring, so the distance from the middle says how deep a domain is
//! classified. Each root's branch keeps its own slice of the disc, and links
//! pull related domains together while every domain keeps its room.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    f64::consts::TAU,
};

use crate::domain::{Domain, DomainId};

/// The least radius of the ring the root domains share.
const CORE: f64 = 150.0;
/// How much wider each level's ring is than the one inside it.
const STEP: f64 = 210.0;
/// The least distance between two domains, room for their names.
const ROOM: f64 = 130.0;
/// Rounds of settling after the first placement.
const ROUNDS: usize = 400;

/// A new place for each domain, in the order given.
///
/// A subdomain with several parents is drawn at the depth of the parent
/// nearest a root and in that parent's slice, and siblings sharing a
/// subdomain sit side by side; its links then draw it towards every parent.
pub fn arrangement(
    domains: &[Domain],
    links: &[(DomainId, DomainId)],
) -> Vec<(DomainId, f64, f64)> {
    if domains.is_empty() {
        return Vec::new();
    }
    let names: HashMap<_, _> =
        domains.iter().map(|domain| (domain.id, domain.name.as_str())).collect();
    let by_name = |a: &DomainId, b: &DomainId| names[a].cmp(names[b]).then(a.cmp(b));
    let mut children = HashMap::<DomainId, Vec<DomainId>>::new();
    for (parent, child) in links {
        children.entry(*parent).or_default().push(*child);
    }
    let hanging: HashSet<_> = links.iter().map(|(_, child)| *child).collect();
    let mut roots: Vec<_> =
        domains.iter().map(|domain| domain.id).filter(|id| !hanging.contains(id)).collect();
    roots.sort_by(by_name);

    // A walk out from the roots gives each domain its depth and the parent
    // whose slice it is drawn in.
    let mut under = HashMap::<DomainId, Vec<DomainId>>::new();
    let mut depth: HashMap<_, _> = roots.iter().map(|root| (*root, 0_usize)).collect();
    let mut queue: VecDeque<_> = roots.iter().copied().collect();
    while let Some(domain) = queue.pop_front() {
        let mut next = children.get(&domain).cloned().unwrap_or_default();
        next.sort_by(by_name);
        for child in next {
            if !depth.contains_key(&child) {
                depth.insert(child, depth[&domain] + 1);
                under.entry(domain).or_default().push(child);
                queue.push_back(child);
            }
        }
    }
    let parents: Vec<_> = under.keys().copied().collect();
    for parent in parents {
        let order = side_by_side(&under[&parent], &under, links);
        under.insert(parent, order);
    }
    let roots = side_by_side(&roots, &under, links);

    // One root takes the middle. Several share a ring just wide enough for them.
    let core = if roots.len() == 1 { 0.0 } else { (roots.len() as f64 * ROOM / TAU).max(CORE) };
    let ring = |level: usize| core + level as f64 * STEP;

    let mut first = Placing { under: &under, ring: &ring, places: HashMap::new() };
    let total: usize = roots.iter().map(|root| first.span(*root)).sum();
    let mut start = -TAU / 4.0;
    for root in &roots {
        let width = TAU * first.span(*root) as f64 / total as f64;
        first.place(*root, 0, (start, width));
        start += width;
    }

    let ids: Vec<_> = domains.iter().map(|domain| domain.id).collect();
    let index: HashMap<_, _> = ids.iter().enumerate().map(|(at, id)| (*id, at)).collect();
    let mut at: Vec<_> = ids.iter().map(|id| first.places[id]).collect();
    let target: Vec<_> = ids.iter().map(|id| ring(depth[id])).collect();
    let edges: Vec<_> = links.iter().map(|(a, b)| (index[a], index[b])).collect();
    settle(&mut at, &target, &edges);
    ids.into_iter().zip(at).map(|(id, (x, y))| (id, x, y)).collect()
}

/// The first placement: each domain in the middle of its slice, on its ring.
struct Placing<'a, F> {
    under: &'a HashMap<DomainId, Vec<DomainId>>,
    ring: &'a F,
    places: HashMap<DomainId, (f64, f64)>,
}

impl<F: Fn(usize) -> f64> Placing<'_, F> {
    /// The outermost domains in a branch, which is the share of the disc it needs.
    fn span(&self, domain: DomainId) -> usize {
        self.under
            .get(&domain)
            .map_or(1, |children| children.iter().map(|child| self.span(*child)).sum::<usize>())
            .max(1)
    }

    fn place(&mut self, domain: DomainId, level: usize, (start, width): (f64, f64)) {
        let (angle, radius) = (start + width / 2.0, (self.ring)(level));
        self.places.insert(domain, (radius * angle.cos(), radius * angle.sin()));
        let Some(children) = self.under.get(&domain) else { return };
        let mut start = start;
        for child in children.clone() {
            let share = width * self.span(child) as f64 / self.span(domain) as f64;
            self.place(child, level + 1, (start, share));
            start += share;
        }
    }
}

/// Settles the first placement: each link draws its two ends into line,
/// domains too close push apart, and each domain keeps to its level's ring
/// `target`. The pulls weaken round by round, so the map comes to rest.
fn settle(at: &mut [(f64, f64)], target: &[f64], edges: &[(usize, usize)]) {
    for round in 0..ROUNDS {
        let cool = 1.0 - round as f64 / ROUNDS as f64;
        for &(a, b) in edges {
            let (dx, dy) = (at[b].0 - at[a].0, at[b].1 - at[a].1);
            let length = dx.hypot(dy).max(1e-6);
            let rest = (target[b] - target[a]).abs().max(STEP * 0.6);
            let pull = 0.06 * cool * (length - rest) / length;
            at[a] = (at[a].0 + dx * pull / 2.0, at[a].1 + dy * pull / 2.0);
            at[b] = (at[b].0 - dx * pull / 2.0, at[b].1 - dy * pull / 2.0);
        }
        for i in 0..at.len() {
            for j in i + 1..at.len() {
                let (dx, dy) = (at[j].0 - at[i].0, at[j].1 - at[i].1);
                let length = dx.hypot(dy);
                if length >= ROOM {
                    continue;
                }
                // Two in one spot part along a direction fixed by their places in the list.
                let (ux, uy) = if length < 1e-6 {
                    let angle = (i * 7 + j * 13) as f64;
                    (angle.cos(), angle.sin())
                } else {
                    (dx / length, dy / length)
                };
                let push = (ROOM - length) / 4.0;
                at[i] = (at[i].0 - ux * push, at[i].1 - uy * push);
                at[j] = (at[j].0 + ux * push, at[j].1 + uy * push);
            }
        }
        for (place, radius) in at.iter_mut().zip(target) {
            let now = place.0.hypot(place.1);
            if now < 1e-6 {
                continue;
            }
            let scale = 1.0 + 0.25 * (radius / now - 1.0);
            *place = (place.0 * scale, place.1 * scale);
        }
    }
}

/// Orders `siblings` so that two whose branches are linked come next to each
/// other, keeping name order otherwise.
fn side_by_side(
    siblings: &[DomainId],
    under: &HashMap<DomainId, Vec<DomainId>>,
    links: &[(DomainId, DomainId)],
) -> Vec<DomainId> {
    if siblings.len() < 3 {
        return siblings.to_vec();
    }
    // Which sibling's branch each domain is drawn in.
    let mut branch = HashMap::new();
    for (index, sibling) in siblings.iter().enumerate() {
        let mut stack = vec![*sibling];
        while let Some(domain) = stack.pop() {
            branch.insert(domain, index);
            stack.extend(under.get(&domain).into_iter().flatten());
        }
    }
    let mut ties = vec![vec![0_usize; siblings.len()]; siblings.len()];
    for (parent, child) in links {
        if let (Some(&a), Some(&b)) = (branch.get(parent), branch.get(child))
            && a != b
        {
            ties[a][b] += 1;
            ties[b][a] += 1;
        }
    }
    let mut left: Vec<usize> = (0..siblings.len()).collect();
    let mut order = vec![left.remove(0)];
    while !left.is_empty() {
        let last = order[order.len() - 1];
        // `max_by_key` keeps the last of equals, so the search runs backwards
        // to keep the first in name order.
        let pick = left
            .iter()
            .enumerate()
            .rev()
            .max_by_key(|(_, index)| ties[last][**index])
            .filter(|(_, index)| ties[last][**index] > 0)
            .map_or(0, |(at, _)| at);
        order.push(left.remove(pick));
    }
    order.into_iter().map(|index| siblings[index]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain(id: i64, name: &str) -> Domain {
        Domain { id: DomainId::new(id), name: name.into(), x: 0.0, y: 0.0 }
    }

    fn link(parent: i64, child: i64) -> (DomainId, DomainId) {
        (DomainId::new(parent), DomainId::new(child))
    }

    fn at(places: &[(DomainId, f64, f64)], id: i64) -> (f64, f64) {
        places
            .iter()
            .find(|(domain, ..)| *domain == DomainId::new(id))
            .map_or((f64::NAN, f64::NAN), |(_, x, y)| (*x, *y))
    }

    fn apart(a: (f64, f64), b: (f64, f64)) -> f64 {
        (a.0 - b.0).hypot(a.1 - b.1)
    }

    fn from_middle(places: &[(DomainId, f64, f64)], id: i64) -> f64 {
        apart(at(places, id), (0.0, 0.0))
    }

    /// Four roots of different sizes, a subdomain under two roots, and a deeper level.
    fn universe() -> (Vec<Domain>, Vec<(DomainId, DomainId)>) {
        let mut domains = vec![
            domain(1, "System"),
            domain(2, "AI"),
            domain(3, "Programming"),
            domain(4, "Agent"),
            domain(5, "AI System"),
        ];
        domains.extend((10..20).map(|id| domain(id, &format!("s{id}"))));
        domains.extend((20..24).map(|id| domain(id, &format!("deep{id}"))));
        domains.extend((30..36).map(|id| domain(id, &format!("p{id}"))));
        let mut links: Vec<_> = (10..20).map(|id| link(1, id)).collect();
        links.extend((20..24).map(|id| link(10 + (id - 20) % 2, id)));
        links.extend((30..36).map(|id| link(3, id)));
        links.extend([link(2, 5), link(1, 5)]);
        (domains, links)
    }

    #[test]
    fn every_domain_gets_a_place_clear_of_the_others() {
        let (domains, links) = universe();

        let places = arrangement(&domains, &links);

        assert_eq!(places.len(), domains.len());
        for (index, (a, ax, ay)) in places.iter().enumerate() {
            for (b, bx, by) in &places[index + 1..] {
                assert!((ax - bx).hypot(ay - by) > 100.0, "{a} and {b} are too close");
            }
        }
    }

    #[test]
    fn roots_gather_at_the_core_and_depth_grows_outwards() {
        let (domains, links) = universe();

        let places = arrangement(&domains, &links);

        let from = |id: DomainId| {
            let (_, x, y) = places.iter().find(|(domain, ..)| *domain == id).copied().unwrap_or((
                id,
                f64::NAN,
                f64::NAN,
            ));
            x.hypot(y)
        };
        for (parent, child) in &links {
            assert!(from(*child) > from(*parent) + STEP * 0.5, "{child} is not beyond {parent}");
        }
        let outermost_root =
            [1, 2, 3, 4].map(|id| from_middle(&places, id)).into_iter().fold(0.0, f64::max);
        for id in (10..20).chain(30..36) {
            assert!(from_middle(&places, id) > outermost_root + STEP * 0.5);
        }
        // The core is small: the roots lie close together.
        for a in [1, 2, 3, 4] {
            for b in [1, 2, 3, 4] {
                assert!(apart(at(&places, a), at(&places, b)) < 2.0 * (CORE + STEP));
            }
        }
    }

    #[test]
    fn a_subdomain_of_two_parents_lies_between_them() {
        let (domains, links) = universe();

        let places = arrangement(&domains, &links);

        // AI System's direction from the middle lies between its two parents'.
        let angle = |id| {
            let (x, y) = at(&places, id);
            y.atan2(x)
        };
        let turn = |a: f64, b: f64| (b - a + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
        let (to_ai, to_system) = (turn(angle(5), angle(2)), turn(angle(5), angle(1)));
        assert!(to_ai * to_system < 0.0, "{to_ai} and {to_system} turn the same way");
    }

    #[test]
    fn siblings_sharing_a_subdomain_go_side_by_side() {
        // Under the root: A to E in name order. X sits under A and D, so D
        // moves next to A, whose other neighbour round the ring is E.
        let names = ["Root", "A", "B", "C", "D", "E", "X"];
        let domains: Vec<_> = (1..).zip(names).map(|(id, name)| domain(id, name)).collect();
        let links =
            [link(1, 2), link(1, 3), link(1, 4), link(1, 5), link(1, 6), link(2, 7), link(5, 7)];

        let places = arrangement(&domains, &links);

        let to_x = |id| apart(at(&places, id), at(&places, 7));
        assert!(to_x(5) < to_x(3) && to_x(5) < to_x(4), "D is not beside A");
    }

    #[test]
    fn a_single_root_sits_in_the_middle() {
        let domains = [domain(1, "Top"), domain(2, "Under")];

        let places = arrangement(&domains, &[link(1, 2)]);

        assert!(from_middle(&places, 1) < 1.0);
        assert!((from_middle(&places, 2) - STEP).abs() < 1.0);
        assert!(arrangement(&[], &[]).is_empty());
    }
}
