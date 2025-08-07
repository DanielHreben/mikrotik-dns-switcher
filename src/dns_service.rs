use std::sync::{Arc, Mutex};
use mikrotik_rs::MikrotikDevice;
use mikrotik_rs::protocol::command::CommandBuilder;
use mikrotik_rs::protocol::CommandResponse;
use tokio::runtime::Runtime;
use crate::config::Config;

/// Get the current DNS server for a specific client IP
pub fn get_current_dns(client_ip: &str, device: Arc<Mutex<MikrotikDevice>>, _config: Arc<Config>) -> Result<String, Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    
    rt.block_on(async {
        // Lock the device for this operation
        let device = device.lock().map_err(|e| format!("Failed to acquire device lock: {}", e))?;
        
        // Query DHCP server leases to find this client's DNS setting
        let command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/print")
            .attribute("where", Some(&format!("address={}", client_ip)))
            .build();
        
        println!("Searching for DHCP lease for client: {}", client_ip);
        let mut response_rx = device.send_command(command).await;
        
        let mut client_dns = String::new();
        let mut lease_found = false;
        
        // Read responses
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Reply(reply)) => {
                    println!("Got lease reply: {:?}", reply.attributes);
                    
                    // Verify this lease actually matches our client IP
                    if let Some(lease_address) = reply.attributes.get("address") {
                        if let Some(addr) = lease_address {
                            if addr == client_ip {
                                lease_found = true;
                                
                                // Look for DNS servers in the lease
                                for (key, value) in &reply.attributes {
                                    if key == "dhcp-option" || key == "dhcp-option-set" {
                                        if let Some(v) = value {
                                            if v.contains("dns") {
                                                client_dns = v.clone();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(CommandResponse::Done(_)) => {
                    println!("DHCP lease query completed");
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    return Err(format!("Error querying DHCP leases: {}", trap.message).into());
                }
                Ok(CommandResponse::Fatal(reason)) => {
                    return Err(format!("Fatal error: {}", reason).into());
                }
                Err(e) => {
                    return Err(format!("Response error: {}", e).into());
                }
            }
        }
        
        if !lease_found {
            // If no specific lease found, get global DNS
            println!("No DHCP lease found for {}, checking global DNS", client_ip);
            
            let command = CommandBuilder::new()
                .command("/ip/dns/print")
                .build();
            
            let mut response_rx = device.send_command(command).await;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Reply(reply)) => {
                        println!("Got global DNS reply: {:?}", reply.attributes);
                        for (key, value) in &reply.attributes {
                            if key == "servers" {
                                if let Some(v) = value {
                                    client_dns = v.clone();
                                }
                            }
                        }
                    }
                    Ok(CommandResponse::Done(_)) => {
                        break;
                    }
                    Ok(CommandResponse::Trap(trap)) => {
                        return Err(format!("Error querying global DNS: {}", trap.message).into());
                    }
                    Ok(CommandResponse::Fatal(reason)) => {
                        return Err(format!("Fatal error: {}", reason).into());
                    }
                    Err(e) => {
                        return Err(format!("Response error: {}", e).into());
                    }
                }
            }
        }
        
        if client_dns.is_empty() {
            Ok(format!("No DNS configured for client {}", client_ip))
        } else {
            Ok(client_dns)
        }
    })
}

/// Set a custom DNS server for a specific client IP by creating a static lease
pub fn set_dns(client_ip: &str, dns_server: &str, device: Arc<Mutex<MikrotikDevice>>, config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    
    rt.block_on(async {
        // Lock the device for this operation
        let device = device.lock().map_err(|e| format!("Failed to acquire device lock: {}", e))?;
        
        // First, check if there's already a static lease managed by us
        let search_command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/print")
            .build();
        
        println!("Searching for existing managed static lease for client: {}", client_ip);
        let mut response_rx = device.send_command(search_command).await;
        
        let mut existing_lease_id = String::new();
        let mut managed_lease_found = false;
        let mut has_dynamic_lease = false;
        let mut dynamic_lease_mac = String::new();
        
        // Read responses to find existing managed lease or dynamic lease
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Reply(reply)) => {
                    if let Some(lease_address) = reply.attributes.get("address") {
                        if let Some(addr) = lease_address {
                            if addr == client_ip {
                                // Check if this is our managed static lease
                                if let Some(comment) = reply.attributes.get("comment") {
                                    if let Some(comment_text) = comment {
                                        if comment_text == &config.app_comment {
                                            // This is our managed static lease
                                            managed_lease_found = true;
                                            if let Some(id_value) = reply.attributes.get(".id") {
                                                if let Some(id) = id_value {
                                                    existing_lease_id = id.clone();
                                                    println!("Found managed static lease ID: {}", existing_lease_id);
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // Check if this is a dynamic lease
                                if let Some(dynamic) = reply.attributes.get("dynamic") {
                                    if let Some(is_dynamic) = dynamic {
                                        if is_dynamic == "true" {
                                            has_dynamic_lease = true;
                                            if let Some(mac) = reply.attributes.get("mac-address") {
                                                if let Some(mac_addr) = mac {
                                                    dynamic_lease_mac = mac_addr.clone();
                                                    println!("Found dynamic lease for {} with MAC: {}", client_ip, dynamic_lease_mac);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(CommandResponse::Done(_)) => {
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    return Err(format!("Error searching leases: {}", trap.message).into());
                }
                Ok(CommandResponse::Fatal(reason)) => {
                    return Err(format!("Fatal error: {}", reason).into());
                }
                Err(e) => {
                    return Err(format!("Response error: {}", e).into());
                }
            }
        }
        
        // Create or update DHCP option for DNS
        let option_name = format!("dns-{}", client_ip.replace(".", "-"));
        
        // Try to create the option first
        let option_command = CommandBuilder::new()
            .command("/ip/dhcp-server/option/add")
            .attribute("name", Some(&option_name))
            .attribute("code", Some("6"))  // DNS server option code
            .attribute("value", Some(&format!("'{}'", dns_server)))
            .attribute("comment", Some(&config.app_comment))
            .build();
        
        let mut response_rx = device.send_command(option_command).await;
        
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Done(_)) => {
                    println!("DNS option created successfully");
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    // Option might already exist, try to update it
                    println!("Option exists, updating: {}", trap.message);
                    
                    let update_option_command = CommandBuilder::new()
                        .command("/ip/dhcp-server/option/set")
                        .attribute("numbers", Some(&option_name))
                        .attribute("value", Some(&format!("'{}'", dns_server)))
                        .build();
                    
                    let mut update_rx = device.send_command(update_option_command).await;
                    while let Some(update_response) = update_rx.recv().await {
                        match update_response {
                            Ok(CommandResponse::Done(_)) => {
                                println!("DNS option updated successfully");
                                break;
                            }
                            Ok(CommandResponse::Trap(update_trap)) => {
                                return Err(format!("Error updating DNS option: {}", update_trap.message).into());
                            }
                            _ => {}
                        }
                    }
                    break;
                }
                Ok(CommandResponse::Fatal(reason)) => {
                    return Err(format!("Fatal error creating DNS option: {}", reason).into());
                }
                Err(e) => {
                    return Err(format!("Response error: {}", e).into());
                }
                _ => {}
            }
        }
        
        if managed_lease_found && !existing_lease_id.is_empty() {
            // Update the existing managed static lease
            println!("Updating existing managed static lease {} with DNS option: {}", existing_lease_id, option_name);
            
            let update_command = CommandBuilder::new()
                .command("/ip/dhcp-server/lease/set")
                .attribute("numbers", Some(&existing_lease_id))
                .attribute("dhcp-option", Some(&option_name))
                .build();
            
            let mut response_rx = device.send_command(update_command).await;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Done(_)) => {
                        println!("Static lease updated successfully for {} with DNS: {}", client_ip, dns_server);
                        return Ok(());
                    }
                    Ok(CommandResponse::Trap(trap)) => {
                        return Err(format!("Error updating static lease: {}", trap.message).into());
                    }
                    Ok(CommandResponse::Fatal(reason)) => {
                        return Err(format!("Fatal error: {}", reason).into());
                    }
                    Err(e) => {
                        return Err(format!("Response error: {}", e).into());
                    }
                    _ => {}
                }
            }
        } else {
            // Need to create a new static lease
            println!("Creating new static lease for client: {}", client_ip);
            
            let mut create_command = CommandBuilder::new()
                .command("/ip/dhcp-server/lease/add")
                .attribute("address", Some(client_ip))
                .attribute("dhcp-option", Some(&option_name))
                .attribute("comment", Some(&config.app_comment));
            
            // If we found a dynamic lease, use its MAC address for the static lease
            if has_dynamic_lease && !dynamic_lease_mac.is_empty() {
                println!("Using MAC address from dynamic lease: {}", dynamic_lease_mac);
                create_command = create_command.attribute("mac-address", Some(&dynamic_lease_mac));
            }
            
            let command = create_command.build();
            let mut response_rx = device.send_command(command).await;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Done(_)) => {
                        println!("Static lease created successfully for {} with DNS: {}", client_ip, dns_server);
                        return Ok(());
                    }
                    Ok(CommandResponse::Trap(trap)) => {
                        return Err(format!("Error creating static lease: {}", trap.message).into());
                    }
                    Ok(CommandResponse::Fatal(reason)) => {
                        return Err(format!("Fatal error: {}", reason).into());
                    }
                    Err(e) => {
                        return Err(format!("Response error: {}", e).into());
                    }
                    _ => {}
                }
            }
        }
        
        Ok(())
    })
}

/// Remove custom DNS for a specific client IP (revert to default) by removing the static lease
pub fn remove_custom_dns(client_ip: &str, device: Arc<Mutex<MikrotikDevice>>, config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    
    rt.block_on(async {
        // Lock the device for this operation
        let device = device.lock().map_err(|e| format!("Failed to acquire device lock: {}", e))?;
        
        // Search for static leases managed by us for this client
        let search_command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/print")
            .attribute("where", Some(&format!("address={} && comment={}", client_ip, config.app_comment)))
            .build();
        
        println!("Searching for managed static lease for client: {}", client_ip);
        let mut response_rx = device.send_command(search_command).await;
        
        let mut lease_id = String::new();
        let mut current_option = String::new();
        let mut managed_lease_found = false;
        
        // Read responses to find existing managed lease
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Reply(reply)) => {
                    println!("Found managed lease: {:?}", reply.attributes);
                    
                    if let Some(lease_address) = reply.attributes.get("address") {
                        if let Some(addr) = lease_address {
                            if addr == client_ip {
                                managed_lease_found = true;
                                
                                if let Some(id_value) = reply.attributes.get(".id") {
                                    if let Some(id) = id_value {
                                        lease_id = id.clone();
                                    }
                                }
                                
                                if let Some(option_value) = reply.attributes.get("dhcp-option") {
                                    if let Some(option) = option_value {
                                        current_option = option.clone();
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(CommandResponse::Done(_)) => {
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    return Err(format!("Error searching managed leases: {}", trap.message).into());
                }
                Ok(CommandResponse::Fatal(reason)) => {
                    return Err(format!("Fatal error: {}", reason).into());
                }
                Err(e) => {
                    return Err(format!("Response error: {}", e).into());
                }
            }
        }
        
        if !managed_lease_found {
            // No managed lease found, nothing to remove
            println!("No managed static lease found for client {}", client_ip);
            return Ok(());
        }
        
        // Remove the static lease managed by us
        println!("Removing managed static lease {} for client {}", lease_id, client_ip);
        let remove_command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/remove")
            .attribute("numbers", Some(&lease_id))
            .build();
        
        let mut response_rx = device.send_command(remove_command).await;
        
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Done(_)) => {
                    println!("Static lease removed successfully for {}", client_ip);
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    return Err(format!("Error removing static lease: {}", trap.message).into());
                }
                Ok(CommandResponse::Fatal(reason)) => {
                    return Err(format!("Fatal error: {}", reason).into());
                }
                Err(e) => {
                    return Err(format!("Response error: {}", e).into());
                }
                _ => {}
            }
        }
        
        // Also remove the DHCP option if it exists
        if !current_option.is_empty() {
            println!("Removing DHCP option: {}", current_option);
            let remove_option_command = CommandBuilder::new()
                .command("/ip/dhcp-server/option/remove")
                .attribute("numbers", Some(&current_option))
                .build();
            
            let mut response_rx = device.send_command(remove_option_command).await;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Done(_)) => {
                        println!("DHCP option {} removed successfully", current_option);
                        break;
                    }
                    Ok(CommandResponse::Trap(trap)) => {
                        // Option might not exist, which is fine
                        println!("Note removing option: {}", trap.message);
                        break;
                    }
                    Ok(CommandResponse::Fatal(reason)) => {
                        return Err(format!("Fatal error removing option: {}", reason).into());
                    }
                    Err(e) => {
                        return Err(format!("Response error: {}", e).into());
                    }
                    _ => {}
                }
            }
        }
        
        Ok(())
    })
}