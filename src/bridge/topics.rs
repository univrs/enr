//! ENR Gossipsub Topics
//!
//! Topic definitions for the ENR gossipsub protocol.

/// ENR topic constants for gossipsub subscription
pub struct EnrTopics;

impl EnrTopics {
    /// Topic for resource gradient broadcasts
    pub const GRADIENT: &'static str = "/enr/gradient/1.0";

    /// Topic for nexus election messages
    pub const ELECTION: &'static str = "/enr/election/1.0";

    /// Topic for credit transfer and sync messages
    pub const CREDIT: &'static str = "/enr/credit/1.0";

    /// Topic for septal gate (circuit breaker) messages
    pub const SEPTAL: &'static str = "/enr/septal/1.0";

    /// Get all ENR topics for subscription
    pub fn all() -> Vec<&'static str> {
        vec![Self::GRADIENT, Self::ELECTION, Self::CREDIT, Self::SEPTAL]
    }

    /// Check if a topic is an ENR topic
    pub fn is_enr_topic(topic: &str) -> bool {
        topic.starts_with("/enr/")
    }

    /// Get the topic type from a topic string
    pub fn topic_type(topic: &str) -> Option<TopicType> {
        match topic {
            Self::GRADIENT => Some(TopicType::Gradient),
            Self::ELECTION => Some(TopicType::Election),
            Self::CREDIT => Some(TopicType::Credit),
            Self::SEPTAL => Some(TopicType::Septal),
            _ => None,
        }
    }
}

/// Topic type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopicType {
    /// Resource gradient broadcasts
    Gradient,
    /// Nexus election messages
    Election,
    /// Credit transfer and sync
    Credit,
    /// Septal gate coordination
    Septal,
}

impl TopicType {
    /// Get the topic string for this type
    pub fn as_str(&self) -> &'static str {
        match self {
            TopicType::Gradient => EnrTopics::GRADIENT,
            TopicType::Election => EnrTopics::ELECTION,
            TopicType::Credit => EnrTopics::CREDIT,
            TopicType::Septal => EnrTopics::SEPTAL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topics_all() {
        let topics = EnrTopics::all();
        assert_eq!(topics.len(), 4);
        assert!(topics.contains(&"/enr/gradient/1.0"));
        assert!(topics.contains(&"/enr/election/1.0"));
        assert!(topics.contains(&"/enr/credit/1.0"));
        assert!(topics.contains(&"/enr/septal/1.0"));
    }

    #[test]
    fn test_is_enr_topic() {
        assert!(EnrTopics::is_enr_topic("/enr/gradient/1.0"));
        assert!(EnrTopics::is_enr_topic("/enr/custom/2.0"));
        assert!(!EnrTopics::is_enr_topic("/other/topic"));
    }

    #[test]
    fn test_topic_type() {
        assert_eq!(
            EnrTopics::topic_type(EnrTopics::GRADIENT),
            Some(TopicType::Gradient)
        );
        assert_eq!(
            EnrTopics::topic_type(EnrTopics::ELECTION),
            Some(TopicType::Election)
        );
        assert_eq!(EnrTopics::topic_type("/unknown"), None);
    }

    #[test]
    fn test_topic_type_as_str() {
        assert_eq!(TopicType::Gradient.as_str(), "/enr/gradient/1.0");
        assert_eq!(TopicType::Credit.as_str(), "/enr/credit/1.0");
    }
}
