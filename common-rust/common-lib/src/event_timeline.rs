// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// EventTimeLine can represent Numerical TimeLine or Event TimeLine
#[derive(PartialEq, Debug, Default)]
pub struct EventTimeLine<T> {
    pub points: Vec<EventTimeLinePoint<T>>,
}

impl<T> EventTimeLine<T> {
    pub fn add_event_info(&mut self, t1: u64, value: T) -> &mut EventTimeLine<T> {
        self.points.push(EventTimeLinePoint { t1, value });
        self
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct EventTimeLinePoint<T> {
    pub t1: u64,
    pub value: T,
}
