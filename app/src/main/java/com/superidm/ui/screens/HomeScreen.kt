package com.superidm.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.superidm.browser.BrowserActivity

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    onNavigateToSettings: () -> Unit = {},
    onNavigateToHistory: () -> Unit = {},
    onNavigateToStreams: () -> Unit = {},
    onNavigateToTorrent: () -> Unit = {},
    onNavigateToAria2: () -> Unit = {},
    onNavigateToStatistics: () -> Unit = {}
) {
    val context = LocalContext.current
    var selectedTab by remember { mutableIntStateOf(0) }
    var speedDialExpanded by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("SuperIDM", style = MaterialTheme.typography.titleLarge) },
                actions = {
                    IconButton(onClick = onNavigateToStatistics) {
                        Icon(Icons.Default.BarChart, contentDescription = "Statistics")
                    }
                    IconButton(onClick = onNavigateToAria2) {
                        Icon(Icons.Default.CloudDownload, contentDescription = "aria2c")
                    }
                    IconButton(onClick = onNavigateToSettings) {
                        Icon(Icons.Default.Settings, contentDescription = "Settings")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    titleContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            )
        },
        bottomBar = {
            NavigationBar {
                NavigationBarItem(
                    selected = selectedTab == 0,
                    onClick = { selectedTab = 0 },
                    icon = { Icon(Icons.Default.Home, contentDescription = "Home") },
                    label = { Text("Home") }
                )
                NavigationBarItem(
                    selected = selectedTab == 1,
                    onClick = {
                        selectedTab = 1
                        onNavigateToHistory()
                    },
                    icon = { Icon(Icons.Default.History, contentDescription = "History") },
                    label = { Text("History") }
                )
                NavigationBarItem(
                    selected = selectedTab == 2,
                    onClick = {
                        selectedTab = 2
                        onNavigateToStreams()
                    },
                    icon = { Icon(Icons.Default.PlayCircleOutline, contentDescription = "Streams") },
                    label = { Text("Streams") }
                )
                NavigationBarItem(
                    selected = selectedTab == 3,
                    onClick = {
                        selectedTab = 3
                        onNavigateToSettings()
                    },
                    icon = { Icon(Icons.Default.Settings, contentDescription = "Settings") },
                    label = { Text("Settings") }
                )
            }
        },
        floatingActionButton = {
            Column(horizontalAlignment = Alignment.End) {
                AnimatedVisibility(visible = speedDialExpanded) {
                    Column(
                        horizontalAlignment = Alignment.End,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.padding(bottom = 16.dp)
                    ) {
                        ExtendedFloatingActionButton(
                            onClick = { speedDialExpanded = false },
                            icon = { Icon(Icons.Default.Link, contentDescription = null) },
                            text = { Text("Add URL") }
                        )
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                onNavigateToTorrent()
                            },
                            icon = { Icon(Icons.Default.InsertDriveFile, contentDescription = null) },
                            text = { Text("Add Torrent") }
                        )
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                BrowserActivity.start(context)
                            },
                            icon = { Icon(Icons.Default.Language, contentDescription = null) },
                            text = { Text("Open Browser") }
                        )
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                onNavigateToStreams()
                            },
                            icon = { Icon(Icons.Default.PlayCircle, contentDescription = null) },
                            text = { Text("Stream URL") }
                        )
                    }
                }
                FloatingActionButton(
                    onClick = { speedDialExpanded = !speedDialExpanded }
                ) {
                    Icon(
                        imageVector = if (speedDialExpanded) Icons.Default.Close else Icons.Default.Add,
                        contentDescription = "Speed Dial"
                    )
                }
            }
        }
    ) { padding ->
        Box(modifier = Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(Icons.Default.Inbox, contentDescription = "Empty", modifier = Modifier.size(64.dp), tint = Color.Gray)
                Spacer(modifier = Modifier.height(16.dp))
                Text("No active downloads", color = Color.Gray)
            }
        }
    }
}
