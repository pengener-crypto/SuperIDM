package com.superidm.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.viewmodel.SettingsViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    viewModel: SettingsViewModel = hiltViewModel(),
    onBack: () -> Unit,
    onNavigateToTheme: () -> Unit,
    onNavigateToAria2: () -> Unit,
    onNavigateToPerSiteRules: () -> Unit,
    onNavigateToScheduler: () -> Unit,
    onNavigateToExport: () -> Unit
) {
    var showResetDialog by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                }
            )
        }
    ) { paddingValues ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .padding(16.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Downloads Section
            SettingsSection("Downloads", Icons.Filled.Download) {
                var maxConcurrent by remember { mutableStateOf(3f) }
                Column(modifier = Modifier.padding(16.dp)) {
                    Text("Max concurrent downloads: ${maxConcurrent.toInt()}")
                    Slider(
                        value = maxConcurrent,
                        onValueChange = { maxConcurrent = it },
                        valueRange = 1f..16f
                    )
                    OutlinedTextField(
                        value = "/storage/emulated/0/Download",
                        onValueChange = {},
                        label = { Text("Default save location") },
                        modifier = Modifier.fillMaxWidth(),
                        trailingIcon = { Icon(Icons.Filled.Folder, contentDescription = "Pick Folder") }
                    )
                    SettingsSwitch("Auto-categorize files", true)
                    SettingsSwitch("Skip duplicate files", false)
                    SettingsSwitch("Auto-start queued downloads", true)
                }
            }

            // Network Section
            SettingsSection("Network", Icons.Filled.Wifi) {
                Column(modifier = Modifier.padding(16.dp)) {
                    SettingsSwitch("WiFi only mode", false)
                    SettingsSwitch("Use dual network bonding", false)
                    
                    var maxSegments by remember { mutableStateOf(8f) }
                    Text("Max segments per download: ${maxSegments.toInt()}")
                    Slider(
                        value = maxSegments,
                        onValueChange = { maxSegments = it },
                        valueRange = 4f..32f
                    )
                    
                    var connectionTimeout by remember { mutableStateOf(30f) }
                    Text("Connection timeout: ${connectionTimeout.toInt()}s")
                    Slider(
                        value = connectionTimeout,
                        onValueChange = { connectionTimeout = it },
                        valueRange = 5f..120f
                    )
                    
                    OutlinedTextField(
                        value = "SuperIDM/1.0",
                        onValueChange = {},
                        label = { Text("Custom User Agent") },
                        modifier = Modifier.fillMaxWidth()
                    )
                }
            }

            // Appearance Section
            SettingsSection("Appearance", Icons.Filled.Palette) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(onClick = onNavigateToTheme)
                        .padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text("Appearance", style = MaterialTheme.typography.bodyLarge)
                        Text("Dark • Material You", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = "Navigate to Theme")
                }
            }

            // Security Section
            SettingsSection("Security", Icons.Filled.Security) {
                Column(modifier = Modifier.padding(16.dp)) {
                    SettingsSwitch("Biometric lock", false)
                    SettingsSwitch("Private mode default", false)
                    SettingsSwitch("Auto-delete after completion", false)
                    SettingsSwitch("Encrypt downloaded files", false)
                }
            }

            // Scheduling Section
            SettingsSection("Scheduling", Icons.Filled.Schedule) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable(onClick = onNavigateToScheduler)
                            .padding(vertical = 8.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text("DND Schedule")
                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = "Navigate to Scheduler")
                    }
                    SettingsSwitch("DND enabled", false)
                }
            }

            // aria2c Section
            SettingsSection("aria2c Integration", Icons.Filled.CloudDownload) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(onClick = onNavigateToAria2)
                        .padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text("aria2c Integration", style = MaterialTheme.typography.bodyLarge)
                        Badge(containerColor = MaterialTheme.colorScheme.primary) { Text("Connected") }
                    }
                    Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = "Navigate to aria2c settings")
                }
            }

            // Advanced Section
            SettingsSection("Advanced", Icons.Filled.SettingsApplications) {
                Column {
                    SettingsNavigationRow("Per-site rules", onNavigateToPerSiteRules)
                    SettingsNavigationRow("Export history", onNavigateToExport)
                    Button(
                        onClick = { },
                        colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.errorContainer, contentColor = MaterialTheme.colorScheme.onErrorContainer),
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp, vertical = 8.dp)
                    ) {
                        Text("Clear completed downloads")
                    }
                    Button(
                        onClick = { showResetDialog = true },
                        colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error, contentColor = MaterialTheme.colorScheme.onError),
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp, vertical = 8.dp)
                    ) {
                        Text("Reset all settings")
                    }
                }
            }
            
            Spacer(modifier = Modifier.height(32.dp))
        }

        if (showResetDialog) {
            AlertDialog(
                onDismissRequest = { showResetDialog = false },
                title = { Text("Reset all settings?") },
                text = { Text("This will revert all preferences to their default values. This action cannot be undone.") },
                confirmButton = {
                    TextButton(onClick = { showResetDialog = false }) { Text("Reset", color = MaterialTheme.colorScheme.error) }
                },
                dismissButton = {
                    TextButton(onClick = { showResetDialog = false }) { Text("Cancel") }
                }
            )
        }
    }
}

@Composable
fun SettingsSection(title: String, icon: ImageVector, content: @Composable () -> Unit) {
    ElevatedCard(modifier = Modifier.fillMaxWidth()) {
        Column {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.padding(16.dp)
            ) {
                Icon(icon, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                Spacer(modifier = Modifier.width(8.dp))
                Text(title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
            }
            Divider()
            content()
        }
    }
}

@Composable
fun SettingsSwitch(text: String, initialChecked: Boolean) {
    var checked by remember { mutableStateOf(initialChecked) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(text)
        Switch(checked = checked, onCheckedChange = { checked = it })
    }
}

@Composable
fun SettingsNavigationRow(text: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(16.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(text)
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
    }
}
