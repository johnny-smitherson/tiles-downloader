pub fn max3(l1: f32, l2: f32, l3: f32) -> f32 {
    if l1 > l2 {
        if l1 > l3 {
            l1
        } else {
            l3
        }
    } else if l2 > l3 {
        l2
    } else {
        l3
    }
}

pub fn min3(l1: f32, l2: f32, l3: f32) -> f32 {
    if l1 < l2 {
        if l1 < l3 {
            l1
        } else {
            l3
        }
    } else if l2 < l3 {
        l2
    } else {
        l3
    }
}
