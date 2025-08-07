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

/// Set a custom DNS server for a specific client IP
pub fn set_dns(client_ip: &str, dns_server: &str, device: Arc<Mutex<MikrotikDevice>>, config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    
    rt.block_on(async {
        // Lock the device for this operation
        let device = device.lock().map_err(|e| format!("Failed to acquire device lock: {}", e))?;
        
        // First, try to find existing DHCP lease for this client
        let search_command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/print")
            .attribute("where", Some(&format!("address={}", client_ip)))
            .build();
        
        println!("Searching for existing DHCP lease for client: {}", client_ip);
        let mut response_rx = device.send_command(search_command).await;
        
        let mut lease_id = String::new();
        let mut lease_found = false;
        
        // Read responses to find existing lease
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Reply(reply)) => {
                    println!("Found existing lease: {:?}", reply.attributes);
                    
                    // Verify this lease actually matches our client IP
                    if let Some(lease_address) = reply.attributes.get("address") {
                        if let Some(addr) = lease_address {
                            println!("Checking lease address: {} vs client: {}", addr, client_ip);
                            if addr == client_ip {
                                lease_found = true;
                                println!("MATCH: Found lease for client {}", client_ip);
                                
                                // Get the lease ID
                                if let Some(id_value) = reply.attributes.get(".id") {
                                    if let Some(id) = id_value {
                                        lease_id = id.clone();
                                        println!("Got lease ID: {}", lease_id);
                                    }
                                }
                            } else {
                                println!("NO MATCH: Lease {} != client {}", addr, client_ip);
                            }
                        }
                    }
                }
                Ok(CommandResponse::Done(_)) => {
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    return Err(format!("Error searching DHCP leases: {}", trap.message).into());
                }
                Ok(CommandResponse::Fatal(reason)) => {
                    return Err(format!("Fatal error: {}", reason).into());
                }
                Err(e) => {
                    return Err(format!("Response error: {}", e).into());
                }
            }
        }
        
        if lease_found && !lease_id.is_empty() {
            // Get full lease details to check if it's manageable
            println!("Querying lease details for ID: {}", lease_id);
            let lease_check_command = CommandBuilder::new()
                .command("/ip/dhcp-server/lease/print")
                .attribute(".id", Some(&lease_id))
                .build();
            
            let mut response_rx = device.send_command(lease_check_command).await;
            let mut existing_option = String::new();
            let mut lease_comment = String::new();
            let mut is_dynamic = false;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Reply(reply)) => {
                        println!("Lease details for conflict check: {:?}", reply.attributes);
                        if let Some(option_value) = reply.attributes.get("dhcp-option") {
                            if let Some(option) = option_value {
                                existing_option = option.clone();
                                println!("Found existing option: {}", existing_option);
                            }
                        }
                        if let Some(comment_value) = reply.attributes.get("comment") {
                            if let Some(comment) = comment_value {
                                lease_comment = comment.clone();
                                println!("Found lease comment: {}", lease_comment);
                            }
                        }
                        if let Some(dynamic_value) = reply.attributes.get("dynamic") {
                            if let Some(dynamic) = dynamic_value {
                                is_dynamic = dynamic == "true";
                                println!("Lease is_dynamic: {} (raw: {})", is_dynamic, dynamic);
                            }
                        }
                    }
                    Ok(CommandResponse::Done(_)) => break,
                    _ => {}
                }
            }
            
            println!("Final values - is_dynamic: {}, lease_comment: '{}', APP_COMMENT: '{}'", is_dynamic, lease_comment, config.app_comment);
            
            // Check if this is a static lease not managed by us
            if !is_dynamic && !lease_comment.is_empty() && lease_comment != config.app_comment {
                println!("CONFLICT DETECTED: Static lease with external comment");
                return Err(format!("Client {} has a static lease with comment '{}' that is not managed by DNS-Switcher. Cannot modify externally managed static leases.", client_ip, lease_comment).into());
            }
            
            // If there's an existing DHCP option, check if it's managed by us
            if !existing_option.is_empty() {
                let expected_option_name = format!("dns-{}", client_ip.replace(".", "-"));
                if existing_option != expected_option_name {
                    // Check if this option has our management comment
                    let option_check_command = CommandBuilder::new()
                        .command("/ip/dhcp-server/option/print")
                        .attribute("where", Some(&format!("name={}", existing_option)))
                        .build();
                    
                    let mut response_rx = device.send_command(option_check_command).await;
                    let mut is_managed_by_us = false;
                    
                    while let Some(response) = response_rx.recv().await {
                        match response {
                            Ok(CommandResponse::Reply(reply)) => {
                                if let Some(comment) = reply.attributes.get("comment") {
                                    if let Some(comment_value) = comment {
                                        if comment_value == &config.app_comment {
                                            is_managed_by_us = true;
                                        }
                                    }
                                }
                            }
                            Ok(CommandResponse::Done(_)) => break,
                            _ => {}
                        }
                    }
                    
                    if !is_managed_by_us {
                        return Err(format!("Client {} already has a custom DNS option '{}' that is not managed by DNS-Switcher. Cannot override it.", client_ip, existing_option).into());
                    }
                }
            }
            
            // If this is a dynamic lease, convert it to static first
            if is_dynamic {
                println!("Converting dynamic lease to static for client: {}", client_ip);
                let make_static_command = CommandBuilder::new()
                    .command("/ip/dhcp-server/lease/make-static")
                    .attribute("numbers", Some(&lease_id))
                    .build();
                
                let mut response_rx = device.send_command(make_static_command).await;
                
                while let Some(response) = response_rx.recv().await {
                    match response {
                        Ok(CommandResponse::Done(_)) => {
                            println!("Successfully converted lease to static");
                            break;
                        }
                        Ok(CommandResponse::Trap(trap)) => {
                            return Err(format!("Error converting lease to static: {}", trap.message).into());
                        }
                        Ok(CommandResponse::Fatal(reason)) => {
                            return Err(format!("Fatal error converting lease to static: {}", reason).into());
                        }
                        Err(e) => {
                            return Err(format!("Response error: {}", e).into());
                        }
                        _ => {}
                    }
                }
                
                // Add our management comment to the newly static lease
                let comment_command = CommandBuilder::new()
                    .command("/ip/dhcp-server/lease/set")
                    .attribute("numbers", Some(&lease_id))
                    .attribute("comment", Some(&config.app_comment))
                    .build();
                
                let mut response_rx = device.send_command(comment_command).await;
                
                while let Some(response) = response_rx.recv().await {
                    match response {
                        Ok(CommandResponse::Done(_)) => {
                            println!("Added management comment to static lease");
                            break;
                        }
                        Ok(CommandResponse::Trap(trap)) => {
                            return Err(format!("Error adding comment to lease: {}", trap.message).into());
                        }
                        Ok(CommandResponse::Fatal(reason)) => {
                            return Err(format!("Fatal error adding comment: {}", reason).into());
                        }
                        Err(e) => {
                            return Err(format!("Response error: {}", e).into());
                        }
                        _ => {}
                    }
                }
            }
            
            // Update existing lease with new DNS
            println!("Updating existing lease {} with DNS: {}", lease_id, dns_server);
            
            // First create a DHCP option for DNS
            let option_command = CommandBuilder::new()
                .command("/ip/dhcp-server/option/add")
                .attribute("name", Some(&format!("dns-{}", client_ip.replace(".", "-"))))
                .attribute("code", Some("6"))  // DNS server option code
                .attribute("value", Some(&format!("'{}'", dns_server)))
                .attribute("comment", Some(&config.app_comment))  // Add our management comment
                .build();
            
            let mut response_rx = device.send_command(option_command).await;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Done(_)) => {
                        println!("DNS option created successfully");
                        break;
                    }
                    Ok(CommandResponse::Trap(trap)) => {
                        // Option might already exist, continue
                        println!("Note: {}", trap.message);
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
            
            // Update lease with the DNS option
            let update_command = CommandBuilder::new()
                .command("/ip/dhcp-server/lease/set")
                .attribute("numbers", Some(&lease_id))
                .attribute("dhcp-option", Some(&format!("dns-{}", client_ip.replace(".", "-"))))
                .build();
            
            let mut response_rx = device.send_command(update_command).await;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Done(_)) => {
                        println!("DHCP lease updated successfully for {} with DNS: {}", client_ip, dns_server);
                        return Ok(());
                    }
                    Ok(CommandResponse::Trap(trap)) => {
                        return Err(format!("Error updating DHCP lease: {}", trap.message).into());
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
            return Err(format!("No DHCP lease found for client IP: {}. Client must have a DHCP lease to set custom DNS.", client_ip).into());
        }
        
        Ok(())
    })
}

/// Remove custom DNS for a specific client IP (revert to default)
pub fn remove_custom_dns(client_ip: &str, device: Arc<Mutex<MikrotikDevice>>, config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    
    rt.block_on(async {
        // Lock the device for this operation
        let device = device.lock().map_err(|e| format!("Failed to acquire device lock: {}", e))?;
        
        // First, find the DHCP lease for this client
        let search_command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/print")
            .attribute("where", Some(&format!("address={}", client_ip)))
            .build();
        
        println!("Searching for existing DHCP lease for client: {}", client_ip);
        let mut response_rx = device.send_command(search_command).await;
        
        let mut lease_id = String::new();
        let mut current_option = String::new();
        let mut lease_comment = String::new();
        let mut lease_found = false;
        
        // Read responses to find existing lease
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Reply(reply)) => {
                    println!("Found existing lease: {:?}", reply.attributes);
                    
                    // Verify this lease actually matches our client IP
                    if let Some(lease_address) = reply.attributes.get("address") {
                        if let Some(addr) = lease_address {
                            if addr == client_ip {
                                lease_found = true;
                                
                                // Get the lease ID, current DHCP option, and comment
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
                                
                                if let Some(comment_value) = reply.attributes.get("comment") {
                                    if let Some(comment) = comment_value {
                                        lease_comment = comment.clone();
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
                    return Err(format!("Error searching DHCP leases: {}", trap.message).into());
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
            return Err(format!("No DHCP lease found for client IP: {}", client_ip).into());
        }
        
        // Check if this is a static lease not managed by us
        if !lease_comment.is_empty() && lease_comment != config.app_comment {
            return Err(format!("Client {} has a static lease with comment '{}' that is not managed by DNS-Switcher. Cannot modify externally managed static leases.", client_ip, lease_comment).into());
        }
        
        if current_option.is_empty() {
            return Ok(()); // No custom DNS to remove
        }
        
        // Check if the current option is managed by our app
        let option_name = format!("dns-{}", client_ip.replace(".", "-"));
        if current_option != option_name {
            // Check if it's another DNS option not managed by us
            let option_search_command = CommandBuilder::new()
                .command("/ip/dhcp-server/option/print")
                .attribute("where", Some(&format!("name={}", current_option)))
                .build();
            
            let mut response_rx = device.send_command(option_search_command).await;
            let mut is_managed_by_us = false;
            
            while let Some(response) = response_rx.recv().await {
                match response {
                    Ok(CommandResponse::Reply(reply)) => {
                        if let Some(comment) = reply.attributes.get("comment") {
                            if let Some(comment_value) = comment {
                                if comment_value == &config.app_comment {
                                    is_managed_by_us = true;
                                }
                            }
                        }
                    }
                    Ok(CommandResponse::Done(_)) => break,
                    Ok(CommandResponse::Trap(_)) => break, // Option not found
                    Ok(CommandResponse::Fatal(reason)) => {
                        return Err(format!("Fatal error: {}", reason).into());
                    }
                    Err(e) => {
                        return Err(format!("Response error: {}", e).into());
                    }
                }
            }
            
            if !is_managed_by_us {
                return Err(format!("Client {} has a custom DNS option '{}' that is not managed by DNS-Switcher. Cannot remove it.", client_ip, current_option).into());
            }
        }
        
        // Remove the DHCP option from the lease
        let update_command = CommandBuilder::new()
            .command("/ip/dhcp-server/lease/set")
            .attribute("numbers", Some(&lease_id))
            .attribute("dhcp-option", Some(""))  // Clear the DHCP option
            .build();
        
        let mut response_rx = device.send_command(update_command).await;
        
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Done(_)) => {
                    println!("DHCP option cleared from lease for {}", client_ip);
                    break;
                }
                Ok(CommandResponse::Trap(trap)) => {
                    return Err(format!("Error updating DHCP lease: {}", trap.message).into());
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
        
        // Now remove the DHCP option itself
        let remove_option_command = CommandBuilder::new()
            .command("/ip/dhcp-server/option/remove")
            .attribute("numbers", Some(&option_name))
            .build();
        
        let mut response_rx = device.send_command(remove_option_command).await;
        
        while let Some(response) = response_rx.recv().await {
            match response {
                Ok(CommandResponse::Done(_)) => {
                    println!("DHCP option {} removed successfully", option_name);
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
        
        Ok(())
    })
}