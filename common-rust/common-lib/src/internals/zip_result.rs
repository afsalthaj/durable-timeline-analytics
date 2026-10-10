// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

/// The values that overlap on one segment when zipping two timelines,
/// remembering which side each value came from.
#[derive(Clone, Debug, PartialEq)]
pub enum ZipResult<'t, T: Clone> {
    Both {
        value: &'t T,
        rest: Box<ZipResult<'t, T>>,
    },
    Singleton {
        value: &'t T,
        side: Side,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Side {
    Left,
    Right,
}

impl<'t, T: Clone> ZipResult<'t, T> {
    // A subtle logic to merge timeline values, by
    // keeping track of the sides in which the values existed.
    // This is to handle the situation of non-existence of values on either side when zipping two timelines, with delayed arrival of values.
    // Example: "cartoon pause" vs "pause cartoon". Let's say pause is play-state-timeline, and cartoon is play-type-timeline.
    // For some reason, we tracked the play type to be cartoon for t1-t2 at time X, and the fact that it was paused came in only the same time period t1-t2 (say at X + 10 just to show its delayed),
    // Given we were trying to zip play_state.zip_with(play_type), the fact of "pause" should be merged towards the left side of value "cartoon".
    // Also another subtility here is, we are not building functions in the recursion by not calling merge at `Both` node.
    pub fn merge<F>(&'t self, f: &F) -> T
    where
        F: Fn(&T, &T) -> T,
    {
        fn go<'t, B, G>(prev: Option<&'t B>, zip_result: &ZipResult<'t, B>, f: &G) -> B
        where
            G: Fn(&B, &B) -> B,
            B: Clone,
        {
            match zip_result {
                ZipResult::Singleton { value, side } => match prev {
                    Some(prev) => match side {
                        Side::Left => f(value, prev),
                        Side::Right => f(prev, value),
                    },
                    None => (*value).clone(),
                },
                ZipResult::Both { value, rest } => go(Some(value), rest, f),
            }
        }

        go(None, self, f)
    }
}
