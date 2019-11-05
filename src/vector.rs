
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Vector2<T>(pub T, pub T);

impl<T> Vector2<T>
where
    T: std::ops::Add<Output=T> + Copy + 'static,
{
    pub fn add(&self, a: impl Into<Self>) -> Self {
        let a = a.into();
        Self(self.0 + a.0, self.1 + a.1)
    }

    pub fn from<U>(Vector2(a, b): Vector2<U>) -> Self
    where
        U: num::cast::AsPrimitive<T>
    {
        Self(a.as_(), b.as_())
    }

    pub fn x(&self) -> T { self.0 }
    pub fn y(&self) -> T { self.1 }
}

impl<T: PartialEq> Eq for Vector2<T> {}

impl<T: Eq + Ord> PartialOrd for Vector2<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Eq + Ord> Ord for Vector2<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.eq(other) {
            std::cmp::Ordering::Equal
        } else if self.1 < other.1 || (self.1 == other.1 && self.0 < other.0) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    }
}

impl<T: Clone> From<&Vector2<T>> for Vector2<T> {
    fn from(a: &Vector2<T>) -> Vector2<T> {
        a.clone()
    }
}

impl<T, X> From<(T, T)> for Vector2<X>
where
    X: From<T>,
{
    fn from((a, b): (T, T)) -> Self {
        Self(a.into(), b.into())
    }
}