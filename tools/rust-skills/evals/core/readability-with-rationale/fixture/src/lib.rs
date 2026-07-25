#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upload {
    pub content_type: Option<String>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmitError {
    EmptyPayload,
    MissingContentType,
    QueueFull,
}

pub fn admit_upload(
    upload: Upload,
    queue: &mut Vec<Upload>,
    max_pending: usize,
) -> Result<(), AdmitError> {
    if !upload.payload.is_empty() {
        if upload.content_type.is_some() {
            // This process-memory bound is intentionally lower than the wire limit.
            if queue.len() < max_pending {
                queue.push(upload);
                Ok(())
            } else {
                Err(AdmitError::QueueFull)
            }
        } else {
            Err(AdmitError::MissingContentType)
        }
    } else {
        Err(AdmitError::EmptyPayload)
    }
}

#[cfg(test)]
mod tests {
    use super::{AdmitError, Upload, admit_upload};

    fn upload(content_type: Option<&str>, payload: &[u8]) -> Upload {
        Upload {
            content_type: content_type.map(str::to_owned),
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn admits_a_complete_upload_when_capacity_remains() {
        let item = upload(Some("application/octet-stream"), b"data");
        let mut queue = Vec::new();

        assert_eq!(admit_upload(item.clone(), &mut queue, 1), Ok(()));
        assert_eq!(queue, vec![item]);
    }

    #[test]
    fn rejects_empty_upload_without_mutating_the_queue() {
        let mut queue = vec![upload(Some("text/plain"), b"existing")];
        let before = queue.clone();

        assert_eq!(
            admit_upload(upload(Some("text/plain"), b""), &mut queue, 2),
            Err(AdmitError::EmptyPayload)
        );
        assert_eq!(queue, before);
    }

    #[test]
    fn rejects_missing_content_type_without_mutating_the_queue() {
        let mut queue = Vec::new();

        assert_eq!(
            admit_upload(upload(None, b"data"), &mut queue, 1),
            Err(AdmitError::MissingContentType)
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn rejects_a_full_queue_without_mutating_it() {
        let existing = upload(Some("text/plain"), b"existing");
        let mut queue = vec![existing.clone()];

        assert_eq!(
            admit_upload(upload(Some("text/plain"), b"new"), &mut queue, 1),
            Err(AdmitError::QueueFull)
        );
        assert_eq!(queue, vec![existing]);
    }
}
