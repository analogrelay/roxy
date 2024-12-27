#[macro_export]
macro_rules! int_newtype {
    ($newtype: ident ($backing_type: ty)) => {
        #[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
        pub struct $newtype($backing_type);

        impl $newtype {
            #[inline]
            #[allow(dead_code)]
            pub const fn get(self) -> $backing_type {
                self.0
            }

            #[inline]
            #[allow(dead_code)]
            pub const fn new(value: $backing_type) -> Self {
                Self(value)
            }
        }

        impl From<$backing_type> for $newtype {
            fn from(value: $backing_type) -> Self {
                Self(value)
            }
        }

        impl From<$newtype> for $backing_type {
            fn from(value: $newtype) -> $backing_type {
                value.0
            }
        }
    };
    ($newtype: ident ($backing_type: ty); atomic $atomic_newtype: ident ($atomic_backing_type: ty)) => {
        int_newtype!($newtype($backing_type));

        pub struct $atomic_newtype($atomic_backing_type);

        impl $atomic_newtype {
            #[inline]
            #[allow(dead_code)]
            pub const fn new(value: $atomic_backing_type) -> Self {
                Self(value)
            }

            #[inline]
            #[allow(dead_code)]
            pub fn load(&self, order: ::core::sync::atomic::Ordering) -> $newtype {
                $newtype::from(self.0.load(order));
            }

            #[inline]
            #[allow(dead_code)]
            pub fn store(&self, value: $newtype, order: ::core::sync::atomic::Ordering) {
                self.0.store(value.into(), order);
            }

            #[inline]
            #[allow(dead_code)]
            pub fn swap(&self, value: $newtype, order: ::core::sync::atomic::Ordering) -> $newtype {
                $newtype::from(self.0.swap(value.into(), order))
            }

            #[inline]
            #[allow(dead_code)]
            pub fn fetch_add(
                &self,
                with: $newtype,
                order: ::core::sync::atomic::Ordering,
            ) -> $newtype {
                $newtype::from(self.0.fetch_add(with.into(), order))
            }

            #[inline]
            #[allow(dead_code)]
            pub fn compare_exchange(
                &self,
                current: $newtype,
                new: $newtype,
                success: ::core::sync::atomic::Ordering,
                failure: ::core::sync::atomic::Ordering,
            ) -> $newtype {
                match self
                    .0
                    .compare_exchange(current.into(), new.into(), success, failure)
                {
                    Ok(value) => $newtype::from(value),
                    Err(value) => $newtype::from(value),
                }
            }

            #[inline]
            #[allow(dead_code)]
            pub fn compare_exchange_weak(
                &self,
                current: $newtype,
                new: $newtype,
                success: ::core::sync::atomic::Ordering,
                failure: ::core::sync::atomic::Ordering,
            ) -> $newtype {
                match self
                    .0
                    .compare_exchange_weak(current.into(), new.into(), success, failure)
                {
                    Ok(value) => $newtype::from(value),
                    Err(value) => $newtype::from(value),
                }
            }
        }

        impl ::core::default::Default for $atomic_newtype {
            #[inline]
            fn default() -> Self {
                Self($atomic_backing_type::new(0))
            }
        }
    };
}
