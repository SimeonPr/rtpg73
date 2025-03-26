use crate::models::{WorldView, Manager, Controller}; 
use driver_rust::elevio::poll::CallButton;

use std::collections::HashMap;
use std::time::{SystemTime, Duration};

use crossbeam_channel as cbc;
use log::{debug, info};

pub fn run(
    id: u8,
    manager_rx: cbc::Receiver<Manager>,
    sender_tx: cbc::Sender<Manager>,
    controller_tx: cbc::Sender<Controller>,
    lights_tx: cbc::Sender<Controller>,
    call_button_rx: cbc::Receiver<CallButton>,
    alarm_rx: cbc::Receiver<u8>
) {
    info!("Manager up and running...");
    let mut world_view = WorldView::init(id);
    let mut network_available = true;
    let mut humble_counter = 5;
    let mut foreign_instants: HashMap<u8, SystemTime> = HashMap::new();

    loop {
        let mut updated = false;

        cbc::select! {
            recv(manager_rx) -> a => {
                let message = a.expect("couldn't get message");
                match message {
                    Manager::Ping(ping_id) => {
                        debug!("Received Ping({})", ping_id);
                        network_available = true;
                        if world_view.id() != ping_id {
                            sender_tx.send(Manager::Pong(world_view.id())).expect("send to sender failed");
                        }
                    },
                    Manager::Pong(_) => {
                        debug!("Received Pong");
                        network_available = true;
                    },
                    Manager::NetworkError => {
                        debug!("Received NetworkError");
                        network_available = false;
                        humble_counter = 5;
                    },
                    Manager::HeartBeat(time_stamp, foreign_world_view) => {
                        debug!("Received WorldView");
                        network_available = true;

                        let foreign_id = foreign_world_view.id();
                        if foreign_id != world_view.id() {
                            let mut is_new = false;

                            if !foreign_instants.contains_key(&foreign_id) {
                                debug!("INSERTING TIMESTAMP");
                                foreign_instants.insert(foreign_id, time_stamp);
                                is_new = true;
                            }

                            let old_ts = foreign_instants.get(&foreign_id).unwrap();
                            if *old_ts >= time_stamp && !is_new {
                                debug!("RECEIVED OLD PACKET");
                            } else {
                                foreign_instants.insert(foreign_id, time_stamp);

                                if humble_counter > 0 {
                                    world_view = world_view.handle_humbly(foreign_world_view);
                                    humble_counter = 0;
                                } else {
                                    let (new_wv, up) = world_view.handle_foreign_world_view(foreign_world_view);
                                    if up {
                                        world_view.compare_world_views(&new_wv);
                                    }
                                    world_view = new_wv;
                                    updated = up;
                                }
                            }
                        } else {
                            debug!("RECEIVED FROM MYSELF");
                        }
                    },
                    Manager::ElevatorState(dirn, behaviour, floor) => {
                        debug!("Received ElevatorState");
                        let (new_wv, up) = world_view.handle_elevator_state(dirn, behaviour, floor);
                        if up {
                            world_view.compare_world_views(&new_wv);
                            world_view = new_wv;
                        }
                        updated = true;
                    },
                    Manager::ClearRequest(floor, should_clear) => {
                        debug!("Received ClearRequest");
                        let (new_wv, up) = world_view.handle_clear_request(floor, &should_clear);
                        if up {
                            world_view.compare_world_views(&new_wv);
                            world_view = new_wv;
                        }
                        updated = up;
                    }
                }
            },
            recv(call_button_rx) -> a => {
                debug!("Received ButtonPress");
                let button_press = a.expect("couldn't get message");
                if humble_counter == 0 && network_available {
                    let (new_wv, up) = world_view.handle_button_press(&button_press);
                    if up {
                        world_view.compare_world_views(&new_wv);
                        world_view = new_wv;
                    }
                    updated = up;
                }
            },
            recv(alarm_rx) -> _ => {
                debug!("Received Alarm");
                debug!("network_available: {}, humble_counter: {}", network_available, humble_counter);

                if !network_available {
                    sender_tx.send(Manager::Ping(world_view.id())).expect("send to sender failed");
                } else if humble_counter > 0 {
                    humble_counter -= 1;
                } else {
                    let (new_wv, up) = world_view.update_states_at_barrier();
                    if up {
                        world_view.compare_world_views(&new_wv);
                        world_view = new_wv;
                    }
                    updated = true;
                }

                for (id, elevator) in world_view.elevators_mut() {
                    if !elevator.has_request() {
                        elevator.set_last_moved(SystemTime::now());
                    }

                    if elevator.has_request() &&
                       elevator.last_moved().elapsed().expect("elapsed() failed") > Duration::from_secs(10) {
                        if elevator.is_working() {
                            info!("Elevator {} is not working", id);
                            updated = true;
                        }
                        elevator.set_is_working(false);
                    }
                }
            }
        }

        if updated {
            if humble_counter <= 0 && network_available {
                let world_view_clone = world_view.clone();
                sender_tx.send(Manager::HeartBeat(SystemTime::now(), world_view_clone)).expect("send to sender failed");
            }

            for elevator in world_view.elevators_mut().values_mut() {
                elevator.set_has_request(false);
            }

            let (controller_reqs, active_elevators) = world_view.assign_requests();

            for (id, elevator) in world_view.elevators_mut() {
                if active_elevators.contains(&(*id as i32)) {
                    elevator.set_has_request(true);
                } else {
                    elevator.set_last_moved(SystemTime::now());
                }
            }

            controller_tx.send(Controller::Requests(controller_reqs)).expect("send to controller failed");
            let lights_reqs = world_view.get_confirmed_requests();
            lights_tx.send(Controller::Requests(lights_reqs)).expect("send to lights failed");
        }
    }
}
