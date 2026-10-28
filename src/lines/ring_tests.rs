use super::*;

#[test]
fn tail_keeps_last() {
    let mut t = Tail::new(2);
    for i in 0..5 {
        t.push(i);
    }
    assert_eq!(t.items(), vec![3, 4]);
    assert_eq!(t.last(), Some(&4));
    assert_eq!(t.len(), 2);
}

#[test]
fn head_keeps_first() {
    let mut h = Head::new(2);
    for i in 0..5 {
        h.push(i);
    }
    assert_eq!(h.items(), vec![0, 1]);
}

#[test]
fn zero_capacity() {
    let mut t = Tail::new(0);
    t.push(1);
    assert!(t.is_empty());
    let mut h = Head::new(0);
    h.push(1);
    assert!(h.is_empty());
}
