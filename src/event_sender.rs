// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

/// Errors that can be returned by EventSender
#[derive(Debug)]
pub enum EventSenderError<Category, EventSubset> {
    /// Error sending the event subset
    EventSendError(::std::sync::mpsc::SendError<EventSubset>),
    /// Error sending the event category
    CategorySendError(::std::sync::mpsc::SendError<Category>),
}

/// This structure is coded to achieve event-subsetting. Receivers in Rust are blocking. One cannot
/// listen to multiple receivers at the same time except by using `try_recv` which again is bad for
/// the same reasons spin-lock based on some sleep is bad (wasting cycles, 50% efficienct on an
/// average etc.). Consider a module that listens to signals from various other modules. Different
/// modules want to talk to this one. So one solution is make a common event set and all senders
/// (registered in all the interested modules) send events from the same set. This is bad for
/// maintenance. Wrong modules might use events not expected to originate from them since it is just
/// one huge event-set. Thus there is a need of event-subsetting and distribute this module-wise so
/// we prevent modules from using wrong events, completely by design and code-mechanics. Also we
/// don't want to spawn threads listening to different receivers (which could force to share
/// ownership and is anyway silly otherwise too). This is what `EventSender` helps to salvage. A
/// simple mechanism that does what a `skip-list` in linked list does. It brings forth a concept of
/// an Umbrella event-category and an event subset. The creator of `EventSender` hard-codes the
/// category for different observers. Each category only links to a particular event-subset and
/// type information of this is put into `EventSender` to during it's construction. Thus when
/// distributed, the modules cannot cheat (do the wrong thing) by trying to fire an event they are
/// not permitted to. Also a single thread listens to many receivers. All problems solved.
///
/// #Examples
///
/// ```
/// # #[macro_use]
/// # extern crate maidsafe_utilities;
/// # fn main() {
///     #[derive(Debug, Clone)]
///     enum EventCategory {
///         Network,
///         UserInterface,
///     }
///
///     #[derive(Debug)]
///     enum NetworkEvent {
///         Connected,
///         Disconnected,
///     }
///
///     #[derive(Debug)]
///     enum UiEvent {
///         CreateDirectory,
///         Terminate,
///     }
///
///     let (ui_event_tx, ui_event_rx) = std::sync::mpsc::channel();
///     let (catergory_tx, catergory_rx) = std::sync::mpsc::channel();
///     let (network_event_tx, network_event_rx) = std::sync::mpsc::channel();
///
///     let ui_event_sender = maidsafe_utilities::event_sender
///                                             ::EventSender::<EventCategory, UiEvent>
///                                             ::new(ui_event_tx,
///                                                   EventCategory::UserInterface,
///                                                   catergory_tx.clone());
///
///     let nw_event_sender = maidsafe_utilities::event_sender
///                                             ::EventSender::<EventCategory, NetworkEvent>
///                                             ::new(network_event_tx,
///                                                   EventCategory::Network,
///                                                   catergory_tx);
///
///     let joiner = thread!("EventListenerThread", move || {
///         for it in catergory_rx.iter() {
///             match it {
///                 EventCategory::Network => {
///                     if let Ok(network_event) = network_event_rx.try_recv() {
///                         match network_event {
///                             NetworkEvent::Connected    => { /* Do Something */ },
///                             NetworkEvent::Disconnected => { /* Do Something */ },
///                         }
///                     }
///                 },
///                 EventCategory::UserInterface => {
///                     if let Ok(ui_event) = ui_event_rx.try_recv() {
///                         match ui_event {
///                             UiEvent::Terminate       => break,
///                             UiEvent::CreateDirectory => { /* Do Something */ },
///                         }
///                     }
///                 }
///             }
///         }
///     });
///
///     let _raii_joiner = maidsafe_utilities::thread::RaiiThreadJoiner::new(joiner);
///
///     assert!(nw_event_sender.send(NetworkEvent::Connected).is_ok());
///     assert!(ui_event_sender.send(UiEvent::CreateDirectory).is_ok());
///     assert!(ui_event_sender.send(UiEvent::Terminate).is_ok());
/// # }
#[derive(Clone)]
pub struct EventSender<Category, EventSubset> {
    event_tx         : ::std::sync::mpsc::Sender<EventSubset>,
    event_category   : Category,
    event_category_tx: ::std::sync::mpsc::Sender<Category>,
}

impl<Category   : ::std::fmt::Debug + Clone,
     EventSubset: ::std::fmt::Debug> EventSender<Category, EventSubset> {
    /// Create a new instance of `EventSender`. Category type, category value and EventSubset type
    /// are baked into `EventSender` to disallow user code from misusing it.
    pub fn new(event_tx         : ::std::sync::mpsc::Sender<EventSubset>,
               event_category   : Category,
               event_category_tx: ::std::sync::mpsc::Sender<Category>) -> EventSender<Category, EventSubset> {
        EventSender {
            event_tx         : event_tx,
            event_category   : event_category,
            event_category_tx: event_category_tx,
        }
    }

    /// Fire an allowed event/signal to the observer.
    pub fn send(&self, event: EventSubset) -> Result<(), EventSenderError<Category, EventSubset>> {
        if let Err(error) = self.event_tx.send(event) {
            return Err(EventSenderError::EventSendError(error))
        }
        if let Err(error) = self.event_category_tx.send(self.event_category.clone()) {
            return Err(EventSenderError::CategorySendError(error))
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn marshall_multiple_events() {
        const TOKEN: u32 = 9876;
        const DIR_NAME: &'static str = "NewDirectory";

        #[derive(Clone, Debug)]
        enum EventCategory {
            Network,
            UserInterface,
        }

        #[derive(Debug)]
        enum NetworkEvent {
            Connected(u32),
            Disconnected,
        }

        #[derive(Debug)]
        enum UiEvent {
            CreateDirectory(String),
            Terminate,
        }

        let (ui_event_tx, ui_event_rx) = ::std::sync::mpsc::channel();
        let (catergory_tx, catergory_rx) = ::std::sync::mpsc::channel();
        let (network_event_tx, network_event_rx) = ::std::sync::mpsc::channel();

        type UiEventSender = EventSender<EventCategory, UiEvent>;
        type NetworkEventSender = EventSender<EventCategory, NetworkEvent>;

        let ui_event_sender = UiEventSender::new(ui_event_tx,
                                                 EventCategory::UserInterface,
                                                 catergory_tx.clone());

        let nw_event_sender = NetworkEventSender::new(network_event_tx,
                                                      EventCategory::Network,
                                                      catergory_tx);

        let joiner = thread!("EventListenerThread", move || {
            for it in catergory_rx.iter() {
                match it {
                    EventCategory::Network => {
                        if let Ok(network_event) = network_event_rx.try_recv() {
                            match network_event {
                                NetworkEvent::Connected(token) => assert_eq!(token, TOKEN),
                                _ => panic!("Shouldn't have received this event: {:?}", network_event),
                            }
                        }
                    },
                    EventCategory::UserInterface => {
                        if let Ok(ui_event) = ui_event_rx.try_recv() {
                            match ui_event {
                                UiEvent::CreateDirectory(name) => assert_eq!(name, DIR_NAME),
                                UiEvent::Terminate => break,
                            }
                        }
                    }
                }
            }
        });

        let _raii_joiner = ::thread::RaiiThreadJoiner::new(joiner);

        assert!(nw_event_sender.send(NetworkEvent::Connected(TOKEN)).is_ok());
        assert!(ui_event_sender.send(UiEvent::CreateDirectory(DIR_NAME.to_string())).is_ok());
        assert!(ui_event_sender.send(UiEvent::Terminate).is_ok());

        ::std::thread::sleep(::std::time::Duration::from_millis(500));

        assert!(ui_event_sender.send(UiEvent::Terminate).is_err());
        assert!(nw_event_sender.send(NetworkEvent::Disconnected).is_err());

        match unwrap_option!(ui_event_sender.send(UiEvent::CreateDirectory(DIR_NAME.to_string()))
                                            .err(),
                             "Return should have evaluated to an error.") {
            EventSenderError::EventSendError(send_err) => {
                match send_err.0 {
                    UiEvent::CreateDirectory(dir_name) => assert_eq!(dir_name, DIR_NAME),
                    _ => panic!("Expected a different event !"),
                }
            },
            _ => panic!("Expected a different error !"),
        }
    }
}
