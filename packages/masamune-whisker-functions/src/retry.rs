use std::future::Future;
use std::time::Duration;

use crate::error::{ApiError, ApiResult};

/// Delays between attempts and which errors are retried.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Delay before each retry. The number of retries is `delays.len()`.
    pub delays: Vec<Duration>,
    /// Adds up to 25% random jitter to each delay.
    pub jitter: bool,
    /// Whether an error is worth retrying.
    pub retry_if: fn(&ApiError) -> bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::none()
    }
}

impl RetryPolicy {
    /// No retries.
    pub fn none() -> Self {
        Self {
            delays: Vec::new(),
            jitter: false,
            retry_if: ApiError::is_transient,
        }
    }

    /// Retries transient errors after each of `delays_ms`.
    pub fn transient(delays_ms: &[u64]) -> Self {
        Self {
            delays: delays_ms
                .iter()
                .map(|ms| Duration::from_millis(*ms))
                .collect(),
            jitter: false,
            retry_if: ApiError::is_transient,
        }
    }

    pub fn with_jitter(mut self) -> Self {
        self.jitter = true;
        self
    }

    pub fn retry_if(mut self, f: fn(&ApiError) -> bool) -> Self {
        self.retry_if = f;
        self
    }
}

/// Runs `f` and retries it according to `policy`.
pub async fn retry<T, F, Fut>(policy: &RetryPolicy, mut f: F) -> ApiResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ApiResult<T>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(value) => return Ok(value),
            Err(error) if attempt < policy.delays.len() && (policy.retry_if)(&error) => {
                let mut delay = policy.delays[attempt];
                if policy.jitter {
                    delay += delay.mul_f64(jitter_fraction() * 0.25);
                }
                log::debug!("retrying after {delay:?}: {error}");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Fails with [`ApiError::Timeout`] when `f` does not finish within `duration`.
pub async fn with_timeout<T>(
    duration: Duration,
    f: impl Future<Output = ApiResult<T>>,
) -> ApiResult<T> {
    tokio::time::timeout(duration, f)
        .await
        .unwrap_or(Err(ApiError::Timeout))
}

/// A number in `[0, 1)` without pulling in a random number crate.
fn jitter_fraction() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    f64::from(nanos % 1000) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test]
    async fn retries_transient_errors_only() {
        let calls = Cell::new(0);
        let policy = RetryPolicy::transient(&[1, 1, 1]);
        let result = retry(&policy, || {
            calls.set(calls.get() + 1);
            async {
                if calls.get() < 3 {
                    Err(ApiError::Timeout)
                } else {
                    Ok(7)
                }
            }
        })
        .await;
        assert_eq!(result, Ok(7));
        assert_eq!(calls.get(), 3);

        calls.set(0);
        let result: ApiResult<()> = retry(&policy, || {
            calls.set(calls.get() + 1);
            async { Err(ApiError::NotSignedIn) }
        })
        .await;
        assert_eq!(result, Err(ApiError::NotSignedIn));
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn times_out() {
        let result: ApiResult<()> = with_timeout(Duration::from_millis(5), async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        })
        .await;
        assert_eq!(result, Err(ApiError::Timeout));
    }
}
