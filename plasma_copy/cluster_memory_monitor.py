from typing import Dict, List, Optional, Any
from dataclasses import dataclass

import ray
from ray._private.internal_api import get_state_from_address, node_stats
from ray.dashboard.utils import node_stats_to_dict

ray_host = "0.0.0.0"

@dataclass
class NodeMemoryStats:
    """Statistics for memory usage on a single node."""
    node_id: str
    node_address: str
    alive: bool
    
    # System memory stats (in bytes)
    total_memory: int
    used_memory: int
    available_memory: int
    
    # Object store stats (in bytes)  
    object_store_total: int
    object_store_used: int
    object_store_available: int
    
    # Additional object store metrics
    num_objects: int
    object_store_fallback_used: int
    spilled_bytes: int
    restored_bytes: int


def get_cluster_nodes_info(address: Optional[str] = None) -> List[Dict[str, Any]]:
    """
    Get information about all nodes in the Ray cluster.
    
    Args:
        address: Ray cluster address (optional, uses current cluster if None)
        
    Returns:
        List of node information dictionaries
    """
    try:
        state = get_state_from_address(address)
        nodes = state.node_table()
        return nodes
    except Exception as e:
        print(f"Error getting cluster nodes info: {e}")
        return []


def get_node_memory_usage(node_manager_address: str, node_manager_port: int) -> Optional[Dict[str, Any]]:
    """
    Get memory and object store usage for a specific node.
    
    Args:
        node_manager_address: Node manager IP address
        node_manager_port: Node manager port
        
    Returns:
        Dictionary containing memory and object store statistics
    """
    try:
        stats_reply = node_stats(
            node_manager_address=node_manager_address,
            node_manager_port=node_manager_port,
            include_memory_info=True
        )
        
        stats_dict = node_stats_to_dict(stats_reply)
        return stats_dict
        
    except Exception as e:
        print(f"Error getting node stats for {node_manager_address}:{node_manager_port}: {e}")
        return None


def parse_node_stats(node_info: Dict[str, Any], node_stats_dict: Optional[Dict[str, Any]]) -> NodeMemoryStats:
    """
    Parse node statistics into a structured format.
    
    Args:
        node_info: Node information from cluster
        node_stats_dict: Node statistics dictionary
        
    Returns:
        NodeMemoryStats object with parsed data
    """
    # Extract basic node info
    node_id = node_info.get("NodeID", "unknown")
    node_address = node_info.get("NodeManagerAddress", "unknown")
    alive = node_info.get("Alive", False)
    
    # Default values for offline nodes
    if not alive or not node_stats_dict:
        return NodeMemoryStats(
            node_id=node_id,
            node_address=node_address,
            alive=alive,
            total_memory=0,
            used_memory=0,
            available_memory=0,
            object_store_total=0,
            object_store_used=0,
            object_store_available=0,
            num_objects=0,
            object_store_fallback_used=0,
            spilled_bytes=0,
            restored_bytes=0
        )
    
    # Parse memory stats - Ray uses different structures depending on version
    total_memory = 0
    used_memory = 0
    available_memory = 0
    
    # Try to get memory info from different possible locations
    if "memoryInfo" in node_stats_dict:
        memory_info = node_stats_dict["memoryInfo"]
        total_memory = memory_info.get("totalMemory", 0)
        used_memory = memory_info.get("usedMemory", 0)
        available_memory = memory_info.get("availableMemory", 0)
    
    # Parse object store stats
    object_store_total = 0
    object_store_used = 0
    object_store_available = 0
    num_objects = 0
    object_store_fallback_used = 0
    spilled_bytes = 0
    restored_bytes = 0
    
    if "storeStats" in node_stats_dict:
        store_stats = node_stats_dict["storeStats"]
        object_store_used = int(store_stats.get("objectStoreBytesUsed", 0))
        object_store_available = int(store_stats.get("objectStoreBytesAvail", 0))
        object_store_total = object_store_used + object_store_available
        num_objects = store_stats.get("numLocalObjects", 0)
        object_store_fallback_used = store_stats.get("objectStoreBytesFallback", 0)
        spilled_bytes = store_stats.get("spilledBytesTotal", 0)
        restored_bytes = store_stats.get("restoredBytesTotal", 0)
    
    return NodeMemoryStats(
        node_id=node_id,
        node_address=node_address,
        alive=alive,
        total_memory=total_memory,
        used_memory=used_memory,
        available_memory=available_memory,
        object_store_total=object_store_total,
        object_store_used=object_store_used,
        object_store_available=object_store_available,
        num_objects=num_objects,
        object_store_fallback_used=object_store_fallback_used,
        spilled_bytes=spilled_bytes,
        restored_bytes=restored_bytes
    )


def get_per_node_memory_usage(address: Optional[str] = None) -> List[NodeMemoryStats]:
    """
    Get memory usage statistics for all nodes in the Ray cluster.
    
    Args:
        address: Ray cluster address (optional)
        
    Returns:
        List of NodeMemoryStats objects for each node
    """
    nodes_info = get_cluster_nodes_info(address)
    if not nodes_info:
        print("No nodes found in the cluster")
        return []
    
    node_stats_list = []
    
    for node_info in nodes_info:
        if not node_info.get("Alive", False):
            # Include offline nodes with empty stats
            stats = parse_node_stats(node_info, None)
            node_stats_list.append(stats)
            continue
            
        node_manager_address = node_info.get("NodeManagerAddress")
        node_manager_port = node_info.get("NodeManagerPort")
        
        if not node_manager_address or not node_manager_port:
            print(f"Skipping node {node_info.get('NodeID', 'unknown')}: missing address/port")
            continue
            
        # Get detailed stats for this node
        node_stats_dict = get_node_memory_usage(node_manager_address, node_manager_port)
        stats = parse_node_stats(node_info, node_stats_dict)
        node_stats_list.append(stats)
    
    return node_stats_list


def format_bytes(bytes_value: int) -> str:
    """Format bytes into human readable string."""
    if bytes_value == 0:
        return "0 B"
    
    for unit in ['B', 'KB', 'MB', 'GB', 'TB']:
        if bytes_value < 1024.0:
            return f"{bytes_value:.1f} {unit}"
        bytes_value /= 1024.0
    return f"{bytes_value:.1f} PB"


def format_percentage(used: int, total: int) -> str:
    """Format usage as percentage."""
    if total == 0:
        return "0%"
    percentage = (used / total) * 100
    return f"{percentage:.1f}%"


def display_cluster_memory_usage(node_stats_list: List[NodeMemoryStats], 
                                include_object_store: bool = True) -> None:
    """
    Display formatted memory usage information for all nodes.
    
    Args:
        node_stats_list: List of node statistics
        include_object_store: Whether to include object store information
    """
    if not node_stats_list:
        print("No node statistics available")
        return
    
    print("=" * 100)
    print("RAY CLUSTER MEMORY USAGE")
    print("=" * 100)
    
    # System Memory Usage Table
    print("\n📊 SYSTEM MEMORY USAGE PER NODE")
    print("-" * 100)
    print(f"{'Node Address':<20} {'Status':<8} {'Total Memory':<15} {'Used Memory':<15} {'Available':<15} {'Usage %':<10}")
    print("-" * 100)
    
    total_system_memory = 0
    total_used_memory = 0
    alive_nodes = 0
    
    for stats in node_stats_list:
        status = "ALIVE" if stats.alive else "DEAD"
        if stats.alive:
            alive_nodes += 1
            total_system_memory += stats.total_memory
            total_used_memory += stats.used_memory
            
        usage_pct = format_percentage(stats.used_memory, stats.total_memory)
        
        print(f"{stats.node_address:<20} {status:<8} {format_bytes(stats.total_memory):<15} "
              f"{format_bytes(stats.used_memory):<15} {format_bytes(stats.available_memory):<15} {usage_pct:<10}")
    
    print("-" * 100)
    print(f"{'CLUSTER TOTAL':<20} {f'{alive_nodes} alive':<8} {format_bytes(total_system_memory):<15} "
          f"{format_bytes(total_used_memory):<15} {format_bytes(total_system_memory - total_used_memory):<15} "
          f"{format_percentage(total_used_memory, total_system_memory):<10}")
    
    if include_object_store:
        # Object Store Usage Table
        print(f"\n🗄️  OBJECT STORE USAGE PER NODE")
        print("-" * 120)
        print(f"{'Node Address':<20} {'Status':<8} {'Total Store':<12} {'Used Store':<12} {'Available':<12} "
              f"{'Usage %':<10} {'Objects':<8} {'Fallback':<12} {'Spilled':<12}")
        print("-" * 120)
        
        total_object_store = 0
        total_object_store_used = 0
        total_objects = 0
        total_fallback = 0
        total_spilled = 0
        
        for stats in node_stats_list:
            status = "ALIVE" if stats.alive else "DEAD"
            if stats.alive:
                total_object_store += stats.object_store_total
                total_object_store_used += stats.object_store_used
                total_objects += stats.num_objects
                total_fallback += stats.object_store_fallback_used
                total_spilled += stats.spilled_bytes
                
            usage_pct = format_percentage(stats.object_store_used, stats.object_store_total)
            
            print(f"{stats.node_address:<20} {status:<8} {format_bytes(stats.object_store_total):<12} "
                  f"{format_bytes(stats.object_store_used):<12} {format_bytes(stats.object_store_available):<12} "
                  f"{usage_pct:<10} {stats.num_objects:<8} {format_bytes(stats.object_store_fallback_used):<12} "
                  f"{format_bytes(stats.spilled_bytes):<12}")
        
        print("-" * 120)
        print(f"{'CLUSTER TOTAL':<20} {f'{alive_nodes} alive':<8} {format_bytes(total_object_store):<12} "
              f"{format_bytes(total_object_store_used):<12} {format_bytes(total_object_store - total_object_store_used):<12} "
              f"{format_percentage(total_object_store_used, total_object_store):<10} {total_objects:<8} "
              f"{format_bytes(total_fallback):<12} {format_bytes(total_spilled):<12}")
    
    print("\n" + "=" * 100)

def main():
    """Main function to demonstrate cluster memory monitoring."""
    print("Ray Cluster Memory Monitor")
    print("-" * 50)
    
    # Check if Ray is initialized
    if not ray.is_initialized():
        print("Ray is not initialized. Starting Ray...")
        ray.init(f"ray://{ray_host}:10001")
    
    try:
        # Get memory usage for all nodes
        print("Collecting memory statistics from all cluster nodes...")
        node_stats_list = get_per_node_memory_usage(f"{ray_host}:6379")
        
        if not node_stats_list:
            print("No cluster nodes found or unable to collect statistics")
            return
        
        # Display the results
        display_cluster_memory_usage(node_stats_list, include_object_store=True)
        
    except Exception as e:
        print(f"Error during monitoring: {e}")
        import traceback
        traceback.print_exc()
    
    finally:
        # Don't shutdown Ray as it might be used by other processes
        print("\nMonitoring complete.")


if __name__ == "__main__":
    main()