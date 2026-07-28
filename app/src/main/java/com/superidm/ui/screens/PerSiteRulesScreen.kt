package com.superidm.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Language
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.data.db.PerSiteRuleEntity
import com.superidm.viewmodel.PerSiteRulesViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PerSiteRulesScreen(
    viewModel: PerSiteRulesViewModel = hiltViewModel(),
    onBack: () -> Unit
) {
    val rules by viewModel.rules.collectAsState(initial = emptyList())
    var showDialog by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Per-Site Rules", color = Color.White) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Default.ArrowBack, contentDescription = "Back", tint = Color.White)
                    }
                },
                modifier = Modifier.background(Brush.horizontalGradient(listOf(Color(0xFF6200EE), Color(0xFF3700B3)))),
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent)
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = { showDialog = true }) {
                Icon(Icons.Default.Add, contentDescription = "Add Rule")
            }
        }
    ) { padding ->
        if (rules.isEmpty()) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding),
                contentAlignment = Alignment.Center
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(Icons.Default.Language, contentDescription = "Globe", modifier = Modifier.size(64.dp), tint = Color.Gray)
                    Spacer(modifier = Modifier.height(16.dp))
                    Text("No rules added", color = Color.Gray)
                }
            }
        } else {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                items(rules, key = { it.domain }) { rule ->
                    val dismissState = rememberSwipeToDismissBoxState(
                        confirmValueChange = {
                            if (it == SwipeToDismissBoxValue.EndToStart) {
                                viewModel.deleteRule(rule.domain)
                                true
                            } else false
                        }
                    )
                    SwipeToDismissBox(
                        state = dismissState,
                        backgroundContent = {
                            Box(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .background(Color.Red)
                                    .padding(horizontal = 16.dp),
                                contentAlignment = Alignment.CenterEnd
                            ) {
                                Text("Delete", color = Color.White)
                            }
                        },
                        content = {
                            ElevatedCard(modifier = Modifier.fillMaxWidth()) {
                                Column(modifier = Modifier.padding(16.dp)) {
                                    Text(rule.domain, style = MaterialTheme.typography.titleMedium)
                                    if (rule.savePath.isNotEmpty()) Text("Save: ${rule.savePath}", style = MaterialTheme.typography.bodySmall)
                                    if (rule.maxSegments > 0) Text("Segments: ${rule.maxSegments}", style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        },
                        enableDismissFromStartToEnd = false
                    )
                }
            }
        }
    }

    if (showDialog) {
        var domain by remember { mutableStateOf("") }
        var savePath by remember { mutableStateOf("") }
        var maxSegments by remember { mutableFloatStateOf(0f) }
        var quality by remember { mutableStateOf("") }
        var proxyHost by remember { mutableStateOf("") }
        var proxyPort by remember { mutableStateOf("") }

        AlertDialog(
            onDismissRequest = { showDialog = false },
            title = { Text("Add Rule") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(value = domain, onValueChange = { domain = it }, label = { Text("Domain") })
                    OutlinedTextField(value = savePath, onValueChange = { savePath = it }, label = { Text("Save Path") })
                    Text("Max Segments: ${maxSegments.toInt()}")
                    Slider(value = maxSegments, onValueChange = { maxSegments = it }, valueRange = 0f..32f, steps = 31)
                    OutlinedTextField(value = quality, onValueChange = { quality = it }, label = { Text("Preferred Quality") })
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedTextField(value = proxyHost, onValueChange = { proxyHost = it }, label = { Text("Proxy Host") }, modifier = Modifier.weight(1f))
                        OutlinedTextField(value = proxyPort, onValueChange = { proxyPort = it }, label = { Text("Port") }, modifier = Modifier.weight(1f))
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    if (domain.isNotEmpty()) {
                        viewModel.addOrUpdateRule(
                            PerSiteRuleEntity(
                                domain = domain,
                                savePath = savePath,
                                maxSegments = maxSegments.toInt(),
                                preferredQuality = quality,
                                proxyHost = proxyHost,
                                proxyPort = proxyPort.toIntOrNull() ?: 0
                            )
                        )
                        showDialog = false
                    }
                }) {
                    Text("Save")
                }
            },
            dismissButton = {
                TextButton(onClick = { showDialog = false }) {
                    Text("Cancel")
                }
            }
        )
    }
}
